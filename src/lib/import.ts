// 导入字段映射（PRD F-14 / U8 裁定）。
//
// U8：CSV 仅映射**同名基础列 + `folder` 列**，其余列归入 `custom_fields`。
// 本模块镜像 Rust `import::bitwarden_csv::KNOWN_COLUMNS`，供导入向导展示列映射预览，
// 保证与后端行为一致。

/** 已知基础列（其余列进入 `custom_fields`）。 */
export const KNOWN_COLUMNS: readonly string[] = [
  'folder',
  'favorite',
  'type',
  'name',
  'notes',
  'login_uri',
  'login_username',
  'login_password',
  'login_totp',
  'fields',
];

/** 列映射结果。 */
export interface HeaderMapping {
  /** 命中已知字段的列（小写、去重、保持出现顺序）。 */
  mapped: string[];
  /** 归入 `custom_fields` 的列。 */
  customFields: string[];
}

/**
 * 由 CSV 表头推导映射关系。
 *
 * 规则与后端一致：表头统一 `trim().toLowerCase()`；已知列归 `mapped`，其余归 `customFields`；
 * 空列名忽略；重复列名只保留首次出现。
 */
export function mapCsvHeader(columns: readonly string[]): HeaderMapping {
  const mapped: string[] = [];
  const customFields: string[] = [];
  const seen = new Set<string>();
  for (const raw of columns) {
    const key = raw.trim().toLowerCase();
    if (key.length === 0 || seen.has(key)) {
      continue;
    }
    seen.add(key);
    if (KNOWN_COLUMNS.includes(key)) {
      mapped.push(key);
    } else {
      customFields.push(key);
    }
  }
  return { mapped, customFields };
}

/** 判定某个键是否为已知基础列。 */
export function isKnownColumn(key: string): boolean {
  return KNOWN_COLUMNS.includes(key.trim().toLowerCase());
}

/** 支持的类型标签（与后端跳过逻辑呼应，供 UI 提示）。 */
export const SUPPORTED_IMPORT_TYPES: readonly string[] = ['login', 'secure note', 'note'];
