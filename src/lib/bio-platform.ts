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

/** 是否处在「平台还没配好、用户该去开通」的状态（§22.4 / D-114）。 */
export function bioShouldOfferSettings(availability: string): boolean {
  return availability === 'not_present' || availability === 'not_configured';
}

/**
 * 是否渲染「一键跳到系统设置」的**按钮**（D-122，2026-10-28 主人拍板）。
 *
 * **只有 Windows**。`ms-settings:signinoptions` 是微软文档化的协议，可靠且会稳定
 * 打开「登录选项」页。
 *
 * Android 撤掉按钮，原因不是没实现，而是**实现了也不可控**：主人真机点击后
 * 「完全没有反应，一点提示都没有」——`startActivity` 没抛异常（抛了就一定会弹红字，
 * 因为 `catch` 里是 `invoke.reject`），说明 Intent 确实发出去了，是 ROM 把
 * `android.settings.BIOMETRIC_ENROLL` 忽略掉了或拉起后立刻 finish。各品牌把生物识别
 * 设置放在完全不同的位置，没有任何 action 能保证命中，也没有办法在客户端检测「到底
 * 跳成功了没有」。留一个点了没反应的按钮，比不给按钮更糟。
 *
 * 所以 Android 改为**纯文案**（`settings.bio_manage_hint_android`）：说清楚去手机的
 * 哪个大类里找，并明说各品牌不同——用户自己两步就能到，比一个假的按钮强。
 */
export function bioCanOpenSettings(platform: BioPlatform): boolean {
  return platform === 'windows';
}

/**
 * 把后端的错误文案 key 按平台改写（D-120）。
 *
 * 只有**含平台名词**的那几条需要改写，搬清单而不是搬规则——清单外的句子一定与平台
 * 无关，宁可漏改（看得见、能补）也不要给每条错误都造两个同义键（翻译漂移、维护翻倍）。
 *
 * 目前清单里两条：
 * - `err.bio_invalidated_enrollment`：Windows 说「Windows Hello 配置已变更」，安卓说
 *   「设备的指纹 / 人脸记录已变更」——照搬 Windows 版就是 D-116 的翻版；
 * - `err.biometric_canceled`：在设置页开启时按「取消」，这句话会**就地显示**在输入框
 *   下方（不是静默），所以它同样看得见。
 *
 * 后端不做这件事：key 由 `passwordms_core::error` 生成，core 层不知道自己在哪个平台；
 * 平台身份 DTO 里已经有了，前端改写是零额外状态的。
 */
const PLATFORM_NOUN_KEYS: readonly string[] = [
  'err.bio_invalidated_enrollment',
  'err.biometric_canceled',
];

export function localizeBioMessageKey(key: string, platform: BioPlatform): string {
  if (platform !== 'android') return key;
  return PLATFORM_NOUN_KEYS.includes(key) ? `${key}_android` : key;
}
