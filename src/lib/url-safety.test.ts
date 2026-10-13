// URL 安全校验（PRD F-05）：仅放行 http/https。

import { describe, expect, it } from 'vitest';

import { isSafeUrl, normalizeAndValidate, UnsafeUrlError } from './url-safety';

describe('normalizeAndValidate', () => {
  it('accepts https and http', () => {
    expect(normalizeAndValidate('https://example.invalid/path')).toBe('https://example.invalid/path');
    expect(normalizeAndValidate('http://example.invalid')).toBe('http://example.invalid/');
  });

  it('prepends https:// when the scheme is missing', () => {
    expect(normalizeAndValidate('example.invalid')).toBe('https://example.invalid/');
    expect(normalizeAndValidate('  example.invalid/login  ')).toBe('https://example.invalid/login');
  });

  it('rejects dangerous schemes', () => {
    expect((): void => void normalizeAndValidate('javascript:alert(1)')).toThrow(UnsafeUrlError);
    expect((): void => void normalizeAndValidate('file:///etc/passwd')).toThrow(UnsafeUrlError);
    expect((): void => void normalizeAndValidate('data:text/html;base64,PHNjcmlwdD4=')).toThrow(
      UnsafeUrlError,
    );
    expect((): void => void normalizeAndValidate('ftp://example.invalid')).toThrow(UnsafeUrlError);
  });

  it('rejects empty input', () => {
    expect((): void => void normalizeAndValidate('   ')).toThrow(UnsafeUrlError);
  });

  it('isSafeUrl mirrors normalizeAndValidate', () => {
    expect(isSafeUrl('https://example.invalid')).toBe(true);
    expect(isSafeUrl('javascript:alert(1)')).toBe(false);
    expect(isSafeUrl('')).toBe(false);
  });
});
