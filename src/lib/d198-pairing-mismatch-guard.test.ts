// D-198 守门：配对时「两头不是同一个保险库」必须**当场**说清楚，且说人话。
//
// 背景（主人真机 1.6.0，2026-11-06 早）：笔记本与公司台式机配对后点「立即同步」，
// 笔记本提示「文件读写错误」、台式机提示「加密操作失败」——两个提示都不指向真正原因。
// 根因与复现见 `crates/passwordms-core/tests/d198_stale_pairing_vk.rs`。
//
// 本文件只钉**前端这一半**（后端一半由 cargo 用例钉住）：
// ① 新错误码 `ERR_VAULT_KEY_MISMATCH` 的兜底 key 真的在四种语言里存在——否则界面会
//    原样印出 `err.vault_key_mismatch`（D-185 的老坑）；
// ② 提示里必须出现界面上真实的两个按钮名（「导出保险库」「从备份恢复」），
//    否则用户读完不知道下一步去哪点；
// ③ 不许退化成「失败了」这类空话（长度下限），也不许借用别的错误码的文案。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES, translate } from './i18n/index';
import { toAppError } from './errors';

const KEY = 'err.vault_key_mismatch';

describe('D-198：两端保险库密钥不一致的提示', () => {
  it('四种语言都有这个键，且都不是原样返回键名（防空跑）', () => {
    for (const locale of LOCALES) {
      expect(hasKey(locale, KEY), `${locale} 缺键 ${KEY}`).toBe(true);
      const text = translate(locale, KEY);
      expect(text, `${locale} 的 ${KEY} 原样返回了键名`).not.toBe(KEY);
      expect(text.length, `${locale} 的 ${KEY} 太短，像占位`).toBeGreaterThan(40);
    }
  });

  it('简中提示必须点名两个真实按钮，用户才知道下一步点哪', () => {
    const text = translate('zh-CN', KEY);
    // 这两个词必须**同时**是界面上真实的按钮文案（从字典里取，不写字面量对照）。
    const exportLabel = translate('zh-CN', 'settings.export_vault');
    const restoreLabel = translate('zh-CN', 'lock.restore_button');
    expect(text).toContain(exportLabel);
    expect(text).toContain(restoreLabel);
    // 还要点名病灶，否则用户以为是网络问题。
    expect(text).toContain('不是同一个保险库');
  });

  it('错误码兜底：后端没带 message_key 时也落到这条提示，不落 err.internal', () => {
    const err = toAppError({ code: 'ERR_VAULT_KEY_MISMATCH' } as never);
    expect(err.messageKey).toBe(KEY);
    // 与「本机库非空」区分开：两条要用户做的事完全不同。
    const empty = toAppError({ code: 'ERR_VK_ADOPT_REQUIRES_EMPTY' } as never);
    expect(empty.messageKey).not.toBe(KEY);
  });
});
