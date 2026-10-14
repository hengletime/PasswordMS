// 全局 UI 状态（Svelte 5 runes）。
//
// 路由、Toast、确认框为内存态；**语言持久化**到 localStorage（D-42：切语言重启即回中文的缺口）。

import { DEFAULT_LOCALE, LOCALES, translate, type Locale } from '../i18n';

/** Toast 类型。 */
export type ToastKind = 'info' | 'success' | 'error';

/** Toast 项。 */
export interface Toast {
  id: number;
  kind: ToastKind;
  messageKey: string;
  params?: Record<string, string | number>;
}

/** 确认框请求。 */
export interface ConfirmRequest {
  titleKey: string;
  messageKey: string;
  params?: Record<string, string | number>;
  resolve: (ok: boolean) => void;
}

/** 顶层路由。 */
export type RouteName = 'lock' | 'create' | 'main';

/** 主界面分区。 */
export type MainSection = 'items' | 'health' | 'trash' | 'settings' | 'sync';

/** 语言持久化键名。 */
const LOCALE_STORAGE_KEY = 'passwordms.locale';

/** 读取持久化语言；非法值 / 隐私模式 / 无 localStorage 时兜底为默认语言。 */
function readStoredLocale(): Locale {
  try {
    if (typeof localStorage === 'undefined') {
      return DEFAULT_LOCALE;
    }
    const raw = localStorage.getItem(LOCALE_STORAGE_KEY);
    return raw !== null && (LOCALES as readonly string[]).includes(raw) ? (raw as Locale) : DEFAULT_LOCALE;
  } catch {
    return DEFAULT_LOCALE;
  }
}

function writeStoredLocale(locale: Locale): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(LOCALE_STORAGE_KEY, locale);
    }
  } catch {
    // 隐私模式 / 存储被禁：语言仅本次会话生效。
  }
}

/** UI store。 */
class UiStore {
  /** 当前语言（持久化）。 */
  locale = $state<Locale>(readStoredLocale());
  /** 当前路由。 */
  route = $state<RouteName>('lock');
  /** 当前主界面分区。 */
  section = $state<MainSection>('items');
  /** Toast 列表（内存态）。 */
  toasts = $state<Toast[]>([]);
  /** 当前确认框（null = 无）。 */
  confirmRequest = $state<ConfirmRequest | null>(null);
  private seq = 0;

  /** 翻译当前语言。 */
  t(key: string, params?: Record<string, string | number>): string {
    return translate(this.locale, key, params);
  }

  /** 切换语言（并持久化）。 */
  setLocale(locale: Locale): void {
    this.locale = locale;
    writeStoredLocale(locale);
  }

  /** 切换路由。 */
  setRoute(route: RouteName): void {
    this.route = route;
  }

  /** 切换分区。 */
  setSection(section: MainSection): void {
    this.section = section;
  }

  /** 弹出 Toast（4 秒后自动消失）。 */
  toast(kind: ToastKind, messageKey: string, params?: Record<string, string | number>): number {
    this.seq += 1;
    const id = this.seq;
    this.toasts = [...this.toasts, { id, kind, messageKey, params }];
    setTimeout((): void => this.dismiss(id), 4000);
    return id;
  }

  /** 关闭 Toast。 */
  dismiss(id: number): void {
    this.toasts = this.toasts.filter((t): boolean => t.id !== id);
  }

  /** 弹出确认框，返回用户选择（Promise 化）。 */
  confirm(
    titleKey: string,
    messageKey: string,
    params?: Record<string, string | number>,
  ): Promise<boolean> {
    return new Promise<boolean>((resolve): void => {
      this.confirmRequest = { titleKey, messageKey, params, resolve };
    });
  }

  /** 结算确认框。 */
  resolveConfirm(ok: boolean): void {
    const req = this.confirmRequest;
    this.confirmRequest = null;
    req?.resolve(ok);
  }
}

/** 单例。 */
export const ui = new UiStore();
