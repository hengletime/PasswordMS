// D-164 守门（二）：Kimi 阶段性审计里三条「一行级」修复的防回退。
//
// 这三条的共同特征不是"难"，而是**同一个不变量只写在一条分支上**：
//   · F-04：就地写头有 `sync_all`，头部扩容整文件重写这条却没有；
//   · F-05：`tombstone`（D-106 修过）先写盘再删内存，`tombstone_blob` 却是反的；
//   · F-06：`syncNow` 成功后会 `vault.load()`，`resolve` 仲裁后却没有。
// 与 D-161（三处视图位只清两位）是同一类错误：**漏一处不报错，只是安静地少做一件事。**
//
// 这类缺陷文本判据已经够用且可靠 —— 它们本来就是"某一行在不在、在谁前面"。
// 真正的掉电/写失败验证属于故障注入套件（审计 S-05），不在本文件范围。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../crates/passwordms-core/src/store/container.rs',
    '../../crates/passwordms-core/src/store/repository.rs',
    './stores/sync.svelte.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

function section(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  if (start < 0) {
    return '';
  }
  const end = text.indexOf(close, start + open.length);
  return end < 0 ? '' : text.slice(start, end + close.length);
}

describe('F-04：头部扩容的整文件重写必须落盘', () => {
  const body = section(
    src('store/container.rs'),
    'let tmp = self.path.with_extension("vault.tmp");',
    'fs::rename(&tmp, &self.path)?;',
  );

  it('⑳ 扩容分支切片成功', () => {
    expect(body).not.toBe('');
    expect(body).toContain('VAULT_MAGIC');
  });

  it('㉑ flush 之后必须 sync_all（只 flush 时掉电会「rename 可见、内容还在缓存」）', () => {
    const flushAt = body.indexOf('file.flush()?;');
    const syncAt = body.indexOf('file.sync_all()?;');
    expect(flushAt, '扩容分支没有 flush').toBeGreaterThan(-1);
    expect(syncAt, '扩容分支缺 sync_all：改主密码时掉电会把库锁死').toBeGreaterThan(-1);
    expect(syncAt, 'sync_all 必须晚于 flush').toBeGreaterThan(flushAt);
    // 顺序也要钉死：先 rename 再 sync 等于没 sync。
    expect(body.indexOf('fs::rename'), 'sync_all 跑在 rename 之后，形同虚设').toBeGreaterThan(
      syncAt,
    );
  });
});

describe('F-05：blob 墓碑必须先写盘再删内存索引', () => {
  const body = section(src('store/repository.rs'), 'fn tombstone_blob(', '\n    }');

  it('㉒ tombstone_blob 切片成功', () => {
    expect(body).not.toBe('');
    expect(body).toContain('FLAG_TOMBSTONE');
  });

  it('㉓ pwrite_frame 必须早于 blobs.remove（否则写失败后内存已忘、磁盘仍活）', () => {
    const writeAt = body.indexOf('pwrite_frame');
    const removeAt = body.indexOf('self.blobs.remove(id)');
    expect(writeAt, '没有写盘').toBeGreaterThan(-1);
    expect(removeAt, '找不到删除内存索引那一行').toBeGreaterThan(-1);
    expect(removeAt, '又回到「先删内存再写盘」：写失败后同一周期不会再重试该 blob').toBeGreaterThan(
      writeAt,
    );
    expect(body, '读索引应该用 get 而不是 remove').toContain('self.blobs.get(id)');
  });
});

describe('F-06：冲突仲裁成功后必须刷新业务视图', () => {
  const body = section(src('stores/sync.svelte.ts'), 'async resolve(', '\n  }');

  it('㉔ resolve 切片成功', () => {
    expect(body).not.toBe('');
    expect(body).toContain('resolveConflict');
  });

  it('㉕ vault.load() 必须出现在 loadConflicts 之前', () => {
    const resolvedAt = body.indexOf('await api.sync.resolveConflict(');
    const reloadAt = body.indexOf('await vault.load();');
    const conflictsAt = body.indexOf('await this.loadConflicts();');
    expect(resolvedAt).toBeGreaterThan(-1);
    expect(reloadAt, '仲裁后没刷新条目列表：冲突没了、数据还是旧的').toBeGreaterThan(-1);
    expect(reloadAt, 'vault.load() 必须晚于仲裁提交').toBeGreaterThan(resolvedAt);
    expect(reloadAt, '应当先刷业务视图再刷冲突箱').toBeLessThan(conflictsAt);
  });
});
