// 生成器参数默认值与「初始参数解析」（D-52：生成器参数跨会话持久化）。
//
// ⚠️ 与 `crates/passwordms-core/src/model/vault.rs` 的 `GeneratorOptions::default()`
//    及 `crate::constants::CHARSET_SYMBOLS` **同名同值**，修改必须同步。

import type { GeneratorOptions } from '../types';

/** 默认生成器参数（与 Rust `GeneratorOptions::default()` 对齐）。 */
export const DEFAULT_GENERATOR_OPTIONS: GeneratorOptions = {
  length: 16,
  uppercase: true,
  lowercase: true,
  digits: true,
  symbols: true,
  exclude_ambiguous: true,
  symbol_set: '+-_.,:',
};

/** `symbol_set` 兜底精编集（与核心层 `CHARSET_SYMBOLS` 一致）。 */
export const SAFE_SYMBOLS = '+-_.,:';

/**
 * 解析生成器初始参数（D-52）。
 *
 * 优先级：`initial`（调用方显式传入，如条目表单场景）>
 *         `stored`（settings store 里跨会话持久化的用户偏好）> 默认值。
 *
 * `symbol_set` 为 null / undefined / 空串时回退精编集 `SAFE_SYMBOLS` —— 与核心层
 * `effective_symbol_set()` 的「None/空 = 用默认精编集」语义一致，保证下游
 * （符号芯片回显、IPC 参数）永远拿到非空符号集。
 */
export function resolveGeneratorOptions(
  initial?: GeneratorOptions | null,
  stored?: GeneratorOptions | null,
): GeneratorOptions {
  const base = initial ?? stored ?? DEFAULT_GENERATOR_OPTIONS;
  const symbolSet =
    base.symbol_set !== null && base.symbol_set !== undefined && base.symbol_set.length > 0
      ? base.symbol_set
      : SAFE_SYMBOLS;
  return { ...base, symbol_set: symbolSet };
}
