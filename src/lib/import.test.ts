// Bitwarden CSV 字段映射（U8）：仅同名基础列 + folder，其余入 custom_fields。

import { describe, expect, it } from 'vitest';

import { isKnownColumn, KNOWN_COLUMNS, mapCsvHeader } from './import';

describe('mapCsvHeader', () => {
  it('maps known columns and routes the rest to custom fields', () => {
    const mapping = mapCsvHeader(['name', 'login_uri', 'custom_x', 'notes', 'my_column']);
    expect(mapping.mapped).toEqual(['name', 'login_uri', 'notes']);
    expect(mapping.customFields).toEqual(['custom_x', 'my_column']);
  });

  it('normalizes case and trims whitespace', () => {
    const mapping = mapCsvHeader(['  Name ', 'LOGIN_URI', 'Folder']);
    expect(mapping.mapped).toEqual(['name', 'login_uri', 'folder']);
    expect(mapping.customFields).toEqual([]);
  });

  it('ignores empty and duplicate columns', () => {
    const mapping = mapCsvHeader(['name', '', 'name', '   ', 'name']);
    expect(mapping.mapped).toEqual(['name']);
    expect(mapping.customFields).toEqual([]);
  });

  it('keeps insertion order of custom fields', () => {
    const mapping = mapCsvHeader(['alpha', 'name', 'beta', 'gamma']);
    expect(mapping.customFields).toEqual(['alpha', 'beta', 'gamma']);
  });

  it('known column set matches the U8 ruling', () => {
    expect(KNOWN_COLUMNS).toEqual([
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
    ]);
    expect(isKnownColumn('LOGIN_PASSWORD')).toBe(true);
    expect(isKnownColumn('secret_extra')).toBe(false);
  });
});
