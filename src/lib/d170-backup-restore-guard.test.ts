// D-170 / Kimi 审计 F-02 + F-03 的源码结构守卫。
//
// 这两条的共同点是「**判定写在 Rust 里，前端测试跑不到**」：
//   - F-02：恢复前只查 magic + 帧流，没查 header_len / 头部 JSON / 算法名。
//     单测能证明「坏文件被拒」（见 core `tests/restore_backup.rs`），但**不能**证明
//     「恢复的接受条件与 `Container::open` 是同一套」——那需要看两者是否共用判定。
//   - F-03：`fs::copy(src, dest)` 直接覆盖不是原子的。要证明「改了顺序」，只能钉接线：
//     候选文件、回读复核、`rename` 三个动作必须同时在场。
//
// 还有一条纯粹的接线风险：**core 产出的 detail 字符串与 src-tauri 的消费分支必须是
// 同一串**。写错一个字母不会编译失败、不会让任何单测变红，只会让用户在「版本不兼容」
// 时看到「这不是备份文件」——于是他去翻找文件，而不是升级应用。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../crates/passwordms-core/src/store/container.rs',
    '../../src-tauri/src/commands/vault_cmd.rs',
    './i18n/zh-CN.ts',
    './i18n/zh-TW.ts',
    './i18n/en-US.ts',
    './i18n/th.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  const hit = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1];
  if (hit === undefined || hit.length === 0) {
    throw new Error(`读不到源码：${suffix}（glob 空集会静默通过，故显式抛错）`);
  }
  return hit;
}

const container = src('container.rs');
const vaultCmd = src('vault_cmd.rs');

/**
 * 截出某个函数的函数体（按大括号配对，能正确处理 `impl` 里的方法）。
 *
 * 不用 `indexOf('\n}\n')`：`Container::open` 是 impl 内的关联函数，体内缩进 `    }`，
 * 直到整个 impl 结束才出现行首 `}`——那样会截到别人身上，断言就变成「随便过」。
 */
function fnBody(source: string, signature: string): string {
  const at = source.indexOf(signature);
  if (at < 0) {
    throw new Error(`找不到 ${signature}`);
  }
  let i = source.indexOf('{', at);
  if (i < 0) {
    throw new Error(`${signature} 之后没有函数体`);
  }
  let depth = 0;
  for (; i < source.length; i += 1) {
    const ch = source[i];
    if (ch === '{') {
      depth += 1;
    } else if (ch === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(at, i + 1);
      }
    }
  }
  throw new Error(`${signature} 的函数体没有闭合`);
}

describe('F-02 恢复的接受条件必须与打开容器一致', () => {
  it('open 与备份预检共用同一套前缀判定（check_prefix），不是两套 if', () => {
    const open = fnBody(container, 'pub fn open(path: &Path)');
    expect(open).toContain('check_prefix(&prefix)');
    // 旧的「就地三条 if」必须彻底消失：留着它，两边就会再次各自长出一套条件。
    expect(open).not.toContain('if &prefix[0..8] != VAULT_MAGIC');
    expect(open).toContain('HeaderDefect::BadMagic');
  });

  it('备份预检覆盖零头长 / 头部 JSON / 可打开性三项', () => {
    const verify = fnBody(container, 'fn verify_backup_container(bytes: &[u8])');
    expect(verify).toContain('check_prefix(&bytes[..CONTAINER_PREFIX_LEN])');
    expect(verify).toContain('serde_json::from_slice(&bytes[CONTAINER_PREFIX_LEN..record_start])');
    expect(verify).toContain('verify_header_usable(&header)');
    expect(verify).toContain('verify_frame_stream(bytes)');
  });

  it('「可打开性」检查算法名与必要密文，缺一即拒', () => {
    const usable = fnBody(container, 'fn verify_header_usable(header: &Vault)');
    expect(usable).toContain('header.kdf.algorithm != KDF_ALGORITHM');
    expect(usable).toContain('header.encryption_scheme != ENCRYPTION_SCHEME');
    expect(usable).toContain('header.kdf_salt.is_empty()');
    expect(usable).toContain('header.wrapped_vault_key.ct.is_empty()');
  });
});

describe('F-03 恢复必须是原子替换', () => {
  it('不再直接 fs::copy 覆盖当前库，改为候选 + rename', () => {
    const restore = fnBody(container, 'pub fn restore_backup_from(src: &Path, dest: &Path)');
    expect(restore).not.toContain('fs::copy(src, dest)');
    expect(restore).toContain('fs::copy(src, &tmp)');
    expect(restore).toContain('fs::rename(&tmp, dest)');
  });

  it('rename 之前必须回读复核 + 刷盘（否则「名字换了、内容还在 cache」）', () => {
    const restore = fnBody(container, 'pub fn restore_backup_from(src: &Path, dest: &Path)');
    const compareAt = restore.indexOf('files_equal(src, &tmp)');
    const syncAt = restore.indexOf('.sync_all()');
    const renameAt = restore.indexOf('fs::rename(&tmp, dest)');
    expect(compareAt).toBeGreaterThan(-1);
    expect(syncAt).toBeGreaterThan(compareAt);
    expect(renameAt).toBeGreaterThan(syncAt);
  });

  it('失败路径要清掉候选文件（不能让半份库留在盘上）', () => {
    const restore = fnBody(container, 'pub fn restore_backup_from(src: &Path, dest: &Path)');
    expect(restore).toContain('if outcome.is_err()');
    expect(restore).toContain('fs::remove_file(&tmp)');
  });

  it('候选文件与目标同目录（跨卷 rename 不是原子操作）', () => {
    const tmpFn = fnBody(container, 'fn restore_temp_path(dest: &Path)');
    expect(tmpFn).toContain('dest.with_file_name');
    expect(tmpFn).toContain('.restore-tmp');
  });
});

describe('F-02 错误码在两端必须是同一串', () => {
  it('core 产出 unsupported_vault_backup，src-tauri 按同一串分叉', () => {
    expect(container).toContain('"unsupported_vault_backup"');
    expect(vaultCmd).toContain('detail == "unsupported_vault_backup"');
  });

  it('版本不兼容有自己的文案键，不是「这不是备份文件」', () => {
    expect(vaultCmd).toContain('"err.backup_unsupported"');
    // 该键必须排在 else 兜底之前，否则永远走不到。
    const at = vaultCmd.indexOf('detail == "unsupported_vault_backup"');
    const fallback = vaultCmd.indexOf('"err.backup_not_vault"');
    expect(at).toBeGreaterThan(-1);
    expect(fallback).toBeGreaterThan(at);
  });

  it('四语言字典都有 backup_unsupported', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      expect(src(`i18n/${lang}.ts`), `${lang} 缺 backup_unsupported`).toContain(
        'backup_unsupported:',
      );
    }
  });
});
