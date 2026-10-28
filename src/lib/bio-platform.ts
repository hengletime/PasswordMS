/**
 * 快速解锁的**平台取词**（`docs/08-biometric-unlock.md` §23）。
 *
 * 为什么需要这一层：同一个功能在两个平台上的名词完全不同——
 * Windows 是「Windows Hello」，Android 是「指纹 / 人脸」；风险说明也不能照搬
 * （Windows 侧的关键弱点是「拿到账户凭据者可绕过」，Android 侧根本不存在这条，
 * 它真正的约束是「新增指纹即让密钥永久失效」）。把 Windows 的文案搬到安卓，
 * 就是 v1.3.1 那个「手机上显示 Windows Hello」的缺陷（D-116）。
 *
 * 平台来源是**后端 DTO 的 `platform` 字段**，不是 `navigator.userAgent`：
 * UA 会随 WebView 版本和厂商 ROM 漂移，后端说出自己是谁才可靠。
 */

/** 当前平台。`other` 表示既不是 Windows 也不是 Android（例如 macOS 构建）。 */
export type BioPlatform = 'windows' | 'android' | 'other';

/** 从 DTO 取平台。缺字段（老版本后端）或有未知值时按 `other` 处理。 */
export function bioPlatformOf(status: { platform?: string } | null | undefined): BioPlatform {
  const p = status?.platform;
  if (p === 'windows') return 'windows';
  if (p === 'android') return 'android';
  return 'other';
}

/**
 * 平台专属文案的键后缀。
 *
 * Windows 沿用原有键（`settings.bio_title` 等，不带后缀），Android 追加 `_android`。
 * `other` 退回 Windows 键：那种平台根本没有适配器（`not_implemented`），整张卡片
 * 不渲染，这些键取不到可见效果——留一个确定的返回值只是为了让类型收敛。
 */
export function bioTextSuffix(platform: BioPlatform): '' | '_android' {
  return platform === 'android' ? '_android' : '';
}

/** 是否应该显示「去系统设置开通」的引导按钮（两个平台都有等价页面，§22.4 / D-114）。 */
export function bioShouldOfferSettings(availability: string): boolean {
  return availability === 'not_present' || availability === 'not_configured';
}
