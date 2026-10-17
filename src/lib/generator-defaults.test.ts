// 生成器初始参数解析（D-52）：符号选择等参数要能从设置 store 里跨会话恢复。

import { describe, expect, it } from 'vitest';

import {
  DEFAULT_GENERATOR_OPTIONS,
  SAFE_SYMBOLS,
  resolveGeneratorOptions,
} from './generator-defaults';

describe('resolveGeneratorOptions (D-52 初始参数解析)', () => {
  it('initial 显式传入时优先于 settings 里持久化的偏好', () => {
    const initial = { ...DEFAULT_GENERATOR_OPTIONS, length: 24, symbol_set: '!@#' };
    const stored = { ...DEFAULT_GENERATOR_OPTIONS, length: 32, symbol_set: '%^&' };
    const resolved = resolveGeneratorOptions(initial, stored);
    expect(resolved.length).toBe(24);
    expect(resolved.symbol_set).toBe('!@#');
  });

  it('未传 initial 时取 settings 里持久化的偏好（符号选择跨会话保留）', () => {
    const stored = { ...DEFAULT_GENERATOR_OPTIONS, length: 20, symbol_set: '!@#$%' };
    expect(resolveGeneratorOptions(undefined, stored)).toEqual(stored);
  });

  it('两者皆缺省时回退默认值', () => {
    expect(resolveGeneratorOptions()).toEqual(DEFAULT_GENERATOR_OPTIONS);
    expect(DEFAULT_GENERATOR_OPTIONS.symbol_set).toBe(SAFE_SYMBOLS);
  });

  it('symbol_set 为 null / undefined / 空串时回退精编集 +-_.,:', () => {
    expect(resolveGeneratorOptions(undefined, { ...DEFAULT_GENERATOR_OPTIONS, symbol_set: null }).symbol_set).toBe(
      SAFE_SYMBOLS,
    );
    expect(resolveGeneratorOptions(undefined, { ...DEFAULT_GENERATOR_OPTIONS, symbol_set: undefined }).symbol_set).toBe(
      SAFE_SYMBOLS,
    );
    expect(resolveGeneratorOptions(undefined, { ...DEFAULT_GENERATOR_OPTIONS, symbol_set: '' }).symbol_set).toBe(
      SAFE_SYMBOLS,
    );
  });

  it('不污染传入对象（返回新对象）', () => {
    const stored = { ...DEFAULT_GENERATOR_OPTIONS, symbol_set: null };
    resolveGeneratorOptions(undefined, stored);
    expect(stored.symbol_set).toBeNull();
  });
});
