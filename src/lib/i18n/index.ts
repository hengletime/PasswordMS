// i18n 入口：字典聚合、键查找与插值（无第三方依赖）。

import { enUS } from './en-US';
import { zhCN } from './zh-CN';
import type { Messages } from './zh-CN';

/** 支持的语言。 */
export type Locale = 'zh-CN' | 'en-US';

/** 语言清单（顺序即切换顺序）。 */
export const LOCALES: readonly Locale[] = ['zh-CN', 'en-US'];

/** 默认语言。 */
export const DEFAULT_LOCALE: Locale = 'zh-CN';

/** 字典表。 */
export const dictionaries: Record<Locale, Messages> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};

/** 按 `a.b.c` 路径取值。 */
function lookup(dict: Record<string, unknown>, key: string): string | undefined {
  let cur: unknown = dict;
  for (const part of key.split('.')) {
    if (typeof cur !== 'object' || cur === null) {
      return undefined;
    }
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

/** 判定某语言是否存在指定键。 */
export function hasKey(locale: Locale, key: string): boolean {
  return lookup(dictionaries[locale] as unknown as Record<string, unknown>, key) !== undefined;
}

/**
 * 翻译：`{n}` 占位符替换；缺失键回退到默认语言，仍缺失则原样返回键名。
 */
export function translate(
  locale: Locale,
  key: string,
  params?: Record<string, string | number>,
): string {
  const dict = dictionaries[locale] as unknown as Record<string, unknown>;
  const fallback = dictionaries[DEFAULT_LOCALE] as unknown as Record<string, unknown>;
  const template = lookup(dict, key) ?? lookup(fallback, key) ?? key;
  if (!params) {
    return template;
  }
  return template.replace(/\{(\w+)\}/g, (_match: string, name: string): string => {
    const value = params[name];
    return value === undefined ? `{${name}}` : String(value);
  });
}

/** 展平字典为点号键清单（用于键完整性校验）。 */
export function flattenKeys(dict: Record<string, unknown>, prefix = ''): string[] {
  const out: string[] = [];
  for (const [k, v] of Object.entries(dict)) {
    const full = prefix ? `${prefix}.${k}` : k;
    if (typeof v === 'string') {
      out.push(full);
    } else if (typeof v === 'object' && v !== null) {
      out.push(...flattenKeys(v as Record<string, unknown>, full));
    }
  }
  return out;
}
