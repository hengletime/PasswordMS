// D-162：两步验证二维码（`otpauth://`）解析。
//
// 职责只有一件：**判断扫进来的这一串我们能不能正确算码**，能就给出 secret，
// 不能就明确说出是哪一项不支持 —— 不做网络请求、不落盘、不碰加密库。
//
// 与 `totp.ts::normalizeTotpInput` 的分工（两者都"解析 otpauth"，但语义不同，不要合并）：
// - `normalizeTotpInput` 服务于**保存**：用户把整串粘进输入框，宽容地抽出 secret，
//   抽不到就原样返回，交给 Rust 侧试算报「输入无效」；
// - `parseOtpauth` 服务于**扫码**：扫码时没有人会去看参数字段，所以我们必须在**入库之前**
//   把 `digits=8` / `period=60` / `algorithm=SHA256` 这类「算出来必然对不上」的码拦下来。
//   手输时代用户会发现「网站要 8 位，这里只出 6 位」；扫码之后这个自我纠正的机会没有了。
//
// 为什么把这些判据放在纯函数里：拒绝路径是本功能最需要被守门的地方（静默错误比报错可怕），
// 而它又完全不需要 DOM / 摄像头 / 文件，离屏可测。

/** 当前版本能正确计算的参数（与 `crates/passwordms-core/src/constants.rs` 逐字一致）。 */
export const SUPPORTED_DIGITS = 6;
export const SUPPORTED_PERIOD_SECS = 30;
export const SUPPORTED_ALGORITHM = 'SHA1';

/** 支持范围内的一次成功解析。 */
export interface OtpauthOk {
  kind: 'ok';
  /** Base32 密钥（已 trim，未做大小写归一 —— 归一化交给保存路径的既有逻辑）。 */
  secret: string;
  /** 发行方（`issuer` 查询参数，或 label 里 `:` 之前那一段）。 */
  issuer: string | null;
  /** 账号（label 里 `:` 之后那一段）。 */
  account: string | null;
}

/** 为什么不能用。 */
export type OtpauthRejectReason =
  | 'not_otpauth'
  | 'hotp'
  | 'no_secret'
  | 'digits'
  | 'period'
  | 'algorithm';

/** 支持范围外（含非两步验证二维码）。 */
export interface OtpauthRejected {
  kind: 'rejected';
  reason: OtpauthRejectReason;
  /** 触发拒绝的具体取值（`digits=8` → `'8'`；`algorithm=SHA256` → `'SHA256'`）。用于文案。 */
  detail: string;
}

export type OtpauthResult = OtpauthOk | OtpauthRejected;

/**
 * 解析 `otpauth://totp/...`。
 *
 * 不用 `new URL()` 拆 host/path：`otpauth:` 是非特殊 scheme，各实现对 authority 的处理
 * 不完全一致（尤其 `GitHub:alice` 这种 label 里带冒号的形态）。这里用一次正则把
 * 「类型 / label / 查询」三段切开，再交给 `URLSearchParams` 解析查询 —— 查询部分才是
 * 需要严格实现的地方。
 */
export function parseOtpauth(raw: string): OtpauthResult {
  const trimmed = raw.trim();
  const matched = /^otpauth:\/\/([^/?#]*)(\/[^?#]*)?(\?[^#]*)?/i.exec(trimmed);
  if (matched === null) {
    return { kind: 'rejected', reason: 'not_otpauth', detail: '' };
  }
  const type = matched[1].toLowerCase();
  if (type === 'hotp') {
    // HOTP 是计数器式（RFC 4226），没有时间维度 —— 存量代码只实现了 TOTP，不能假装支持。
    return { kind: 'rejected', reason: 'hotp', detail: 'hotp' };
  }
  if (type !== 'totp') {
    return { kind: 'rejected', reason: 'not_otpauth', detail: type };
  }

  const params = new URLSearchParams(matched[3] ?? '');

  const secret = (params.get('secret') ?? '').trim();
  if (secret.length === 0) {
    return { kind: 'rejected', reason: 'no_secret', detail: '' };
  }

  // 参数缺失时按 RFC 6238 默认值（6 位 / 30 秒 / SHA1）——**"没写"不是"不支持"**。
  // 只有明确写成别的值才拒绝：默认值省略是最常见的正常形态。
  const digitsRaw = (params.get('digits') ?? '').trim();
  if (digitsRaw !== '' && Number(digitsRaw) !== SUPPORTED_DIGITS) {
    return { kind: 'rejected', reason: 'digits', detail: digitsRaw };
  }
  const periodRaw = (params.get('period') ?? '').trim();
  if (periodRaw !== '' && Number(periodRaw) !== SUPPORTED_PERIOD_SECS) {
    return { kind: 'rejected', reason: 'period', detail: periodRaw };
  }
  const algoRaw = (params.get('algorithm') ?? '').trim();
  if (algoRaw !== '' && algoRaw.toUpperCase() !== SUPPORTED_ALGORITHM) {
    return { kind: 'rejected', reason: 'algorithm', detail: algoRaw };
  }

  const label = decodeLabel(matched[2] ?? '');
  const issuerParam = (params.get('issuer') ?? '').trim();
  const colon = label.indexOf(':');
  const labelIssuer = colon >= 0 ? label.slice(0, colon).trim() : '';
  const labelAccount = colon >= 0 ? label.slice(colon + 1).trim() : label.trim();

  return {
    kind: 'ok',
    secret,
    // 查询参数优先：Google Authenticator 等生成的码里两者都有，参数是权威值；
    // label 前缀只是给人看的显示名（某些站点两者不一致）。
    issuer: issuerParam.length > 0 ? issuerParam : labelIssuer.length > 0 ? labelIssuer : null,
    account: labelAccount.length > 0 ? labelAccount : null,
  };
}

/**
 * 解码 label（`GitHub:alice%40example.com` → `GitHub:alice@example.com`）。
 *
 * 单个畸形转义不该让整张码被判成非法：解不出来就用原串，最坏情况是标题里带了个 `%`。
 */
function decodeLabel(pathPart: string): string {
  const raw = pathPart.startsWith('/') ? pathPart.slice(1) : pathPart;
  if (raw.length === 0) {
    return '';
  }
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
}

/** 这条判定发生在哪个入口 —— 只影响措辞（"二维码" vs "链接"），不影响判据。 */
export type OtpauthScene = 'scan' | 'paste';

/**
 * 拒绝原因 → i18n 键 + 文案参数。
 *
 * 放在这里而不是编辑器里：拒绝文案与判据必须同源，否则「判据说 period 不对、文案说 digits」
 * 这种错配没人会去核对（D-150/D-151 那一族）。
 *
 * `scene` 只换措辞：扫码路径说「这不是两步验证**二维码**」，粘贴路径说「这不是两步验证**链接**」。
 * 判据与键的映射关系只有这一份 —— 两个入口共用同一条判定，是 D-162 的核心要求：
 * 同一份参数（digits=8）不该因为"从哪儿进来"而一个被拒、一个被放行。
 */
export function rejectMessage(
  rejected: OtpauthRejected,
  scene: OtpauthScene = 'scan',
): { key: string; params?: Record<string, string> } {
  switch (rejected.reason) {
    case 'hotp':
      return { key: scene === 'scan' ? 'editor.scan_hotp' : 'editor.link_hotp' };
    case 'no_secret':
      return { key: 'editor.scan_no_secret' };
    case 'digits':
      // 「这个网站用 8 位验证码」两种入口都通顺，不需要分场景。
      return { key: 'editor.scan_digits', params: { n: rejected.detail } };
    case 'period':
      return { key: 'editor.scan_period', params: { n: rejected.detail } };
    case 'algorithm':
      return { key: 'editor.scan_algorithm', params: { algo: rejected.detail } };
    case 'not_otpauth':
    default:
      // 刻意不回显扫码内容：那可能是任意 URL 或别的二维码里的私密串，
      // 弹在界面上一来没必要，二来等于把「扫到什么」泄露给旁人。
      return { key: scene === 'scan' ? 'editor.scan_not_otpauth' : 'editor.link_not_otpauth' };
  }
}
