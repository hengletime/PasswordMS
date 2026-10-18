// 前端错误码与统一错误处理（架构文档 §9.2）。
//
// 错误码字符串与 Rust `error::codes` **同名同值**；文案 key 与 `CoreError::message_key()` 一致。

import type { IpcErrorShape } from '../types';

/** 错误码常量表。 */
export const ErrorCode = {
  ERR_UNLOCK_FAILED: 'ERR_UNLOCK_FAILED',
  ERR_NO_VAULT: 'ERR_NO_VAULT',
  ERR_VAULT_EXISTS: 'ERR_VAULT_EXISTS',
  ERR_WEAK_MASTER_PASSWORD: 'ERR_WEAK_MASTER_PASSWORD',
  ERR_NOT_UNLOCKED: 'ERR_NOT_UNLOCKED',
  ERR_ITEM_NOT_FOUND: 'ERR_ITEM_NOT_FOUND',
  ERR_FOLDER_NOT_FOUND: 'ERR_FOLDER_NOT_FOUND',
  ERR_FOLDER_NAME_INVALID: 'ERR_FOLDER_NAME_INVALID',
  ERR_VALIDATION: 'ERR_VALIDATION',
  ERR_ATTACHMENT_TOO_LARGE: 'ERR_ATTACHMENT_TOO_LARGE',
  ERR_VAULT_CAPACITY_EXCEEDED: 'ERR_VAULT_CAPACITY_EXCEEDED',
  ERR_UNSAFE_URL: 'ERR_UNSAFE_URL',
  ERR_IMPORT_PARSE: 'ERR_IMPORT_PARSE',
  ERR_IMPORT_ENCRYPTED_EXPORT: 'ERR_IMPORT_ENCRYPTED_EXPORT',
  ERR_PAIR_TOKEN_INVALID: 'ERR_PAIR_TOKEN_INVALID',
  ERR_PAIR_TOKEN_EXPIRED: 'ERR_PAIR_TOKEN_EXPIRED',
  ERR_PAIR_TOKEN_USED: 'ERR_PAIR_TOKEN_USED',
  ERR_NOT_PAIRED: 'ERR_NOT_PAIRED',
  ERR_REPLAY_DETECTED: 'ERR_REPLAY_DETECTED',
  ERR_FINGERPRINT_MISMATCH: 'ERR_FINGERPRINT_MISMATCH',
  ERR_SYNC_ABORTED: 'ERR_SYNC_ABORTED',
  ERR_CONFLICT_EXISTS: 'ERR_CONFLICT_EXISTS',
  ERR_VK_ADOPT_REQUIRES_EMPTY: 'ERR_VK_ADOPT_REQUIRES_EMPTY',
  ERR_BLOB_MISSING: 'ERR_BLOB_MISSING',
  ERR_BLOB_INCOMPLETE: 'ERR_BLOB_INCOMPLETE',
  ERR_BLOB_CAPACITY: 'ERR_BLOB_CAPACITY',
  ERR_IO: 'ERR_IO',
  ERR_CRYPTO: 'ERR_CRYPTO',
  ERR_INTERNAL: 'ERR_INTERNAL',
} as const;

/** 错误码联合类型。 */
export type ErrorCode = (typeof ErrorCode)[keyof typeof ErrorCode];

/** 错误码 → 文案 key 前缀兜底（后端未给 message_key 时使用）。 */
const FALLBACK_MESSAGE_KEY: Record<string, string> = {
  [ErrorCode.ERR_UNLOCK_FAILED]: 'err.unlock_failed',
  [ErrorCode.ERR_NO_VAULT]: 'err.no_vault',
  [ErrorCode.ERR_VAULT_EXISTS]: 'err.vault_exists',
  [ErrorCode.ERR_WEAK_MASTER_PASSWORD]: 'err.weak_password',
  [ErrorCode.ERR_NOT_UNLOCKED]: 'err.not_unlocked',
  [ErrorCode.ERR_ITEM_NOT_FOUND]: 'err.item_not_found',
  [ErrorCode.ERR_FOLDER_NOT_FOUND]: 'err.folder_not_found',
  [ErrorCode.ERR_FOLDER_NAME_INVALID]: 'err.folder_name',
  [ErrorCode.ERR_VALIDATION]: 'err.validation',
  [ErrorCode.ERR_ATTACHMENT_TOO_LARGE]: 'err.attachment_too_large',
  [ErrorCode.ERR_VAULT_CAPACITY_EXCEEDED]: 'err.vault_capacity',
  [ErrorCode.ERR_UNSAFE_URL]: 'err.unsafe_url',
  [ErrorCode.ERR_IMPORT_PARSE]: 'err.import_parse',
  [ErrorCode.ERR_IMPORT_ENCRYPTED_EXPORT]: 'err.import_encrypted',
  [ErrorCode.ERR_PAIR_TOKEN_INVALID]: 'err.pair_token_invalid',
  [ErrorCode.ERR_PAIR_TOKEN_EXPIRED]: 'err.pair_token_expired',
  [ErrorCode.ERR_PAIR_TOKEN_USED]: 'err.pair_token_used',
  [ErrorCode.ERR_NOT_PAIRED]: 'err.not_paired',
  [ErrorCode.ERR_REPLAY_DETECTED]: 'err.replay',
  [ErrorCode.ERR_FINGERPRINT_MISMATCH]: 'err.fingerprint',
  [ErrorCode.ERR_SYNC_ABORTED]: 'err.sync_aborted',
  [ErrorCode.ERR_CONFLICT_EXISTS]: 'err.conflict',
  [ErrorCode.ERR_VK_ADOPT_REQUIRES_EMPTY]: 'err.vk_adopt_requires_empty',
  [ErrorCode.ERR_BLOB_MISSING]: 'err.blob_missing',
  [ErrorCode.ERR_BLOB_INCOMPLETE]: 'err.blob_incomplete',
  [ErrorCode.ERR_BLOB_CAPACITY]: 'err.blob_capacity',
  [ErrorCode.ERR_IO]: 'err.io',
  [ErrorCode.ERR_CRYPTO]: 'err.crypto',
  [ErrorCode.ERR_INTERNAL]: 'err.internal',
};

/** 应用统一错误。 */
export class AppError extends Error {
  readonly code: string;
  readonly messageKey: string;
  readonly recordCount?: number;

  constructor(code: string, messageKey: string, recordCount?: number) {
    super(messageKey);
    this.name = 'AppError';
    this.code = code;
    this.messageKey = messageKey;
    this.recordCount = recordCount;
  }
}

/** 判定是否为后端错误形态。 */
function isIpcErrorShape(value: unknown): value is IpcErrorShape {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  const v = value as Record<string, unknown>;
  return typeof v.code === 'string';
}

/**
 * 把任意抛出物归一化为 {@link AppError}。
 *
 * 说明：Tauri 的 `invoke` 在命令返回 `Err(IpcError)` 时，会以 `{ code, message_key }`
 * 对象形式 reject，故这里优先识别该形态。
 */
export function toAppError(err: unknown): AppError {
  if (err instanceof AppError) {
    return err;
  }
  if (isIpcErrorShape(err)) {
    const key = err.message_key || FALLBACK_MESSAGE_KEY[err.code] || 'err.internal';
    return new AppError(err.code, key, err.record_count);
  }
  if (err instanceof Error) {
    return new AppError(ErrorCode.ERR_INTERNAL, 'err.internal');
  }
  return new AppError(ErrorCode.ERR_INTERNAL, 'err.internal');
}
