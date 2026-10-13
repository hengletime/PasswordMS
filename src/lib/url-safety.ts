// URL 协议白名单校验（PRD F-05）。
//
// 与 `crates/passwordms-core/src/url_safety.rs` 行为对齐：仅放行 http/https；
// 缺少协议头自动补 `https://`；其余协议（`javascript:` / `file:` 等）一律拒绝。

/** URL 不安全时抛出的错误。 */
export class UnsafeUrlError extends Error {
  constructor() {
    super('unsafe url');
    this.name = 'UnsafeUrlError';
  }
}

/**
 * 归一化并校验 URL，返回可安全打开的绝对地址。
 *
 * @throws {UnsafeUrlError} 输入为空、无法解析或协议不在白名单内。
 */
export function normalizeAndValidate(input: string): string {
  const candidate = input.trim();
  if (candidate.length === 0) {
    throw new UnsafeUrlError();
  }
  // 缺少协议头时补 https://（与 Rust 侧启发式一致）。
  const withScheme =
    candidate.includes('://') || candidate.includes(':') ? candidate : `https://${candidate}`;

  let parsed: URL;
  try {
    parsed = new URL(withScheme);
  } catch {
    throw new UnsafeUrlError();
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new UnsafeUrlError();
  }
  return parsed.toString();
}

/** 判定 URL 是否可安全打开（不抛异常版本）。 */
export function isSafeUrl(input: string): boolean {
  try {
    normalizeAndValidate(input);
    return true;
  } catch {
    return false;
  }
}
