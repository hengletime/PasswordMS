/**
 * 密码强度分级（D-100，2026-10-25）。
 *
 * 旧公式（字符类数 + 长度加成）在 4 类全开时恒为满分——生成器里拖长度质量不变
 * （主人实测反馈）。现改为**信息熵估算**：`bits = len × log2(实际出现的字符池)`，
 * 长度与字符池连续贡献，每档长度都可感知。
 *
 * 阈值（对齐常用强度分级口径）：
 * - `< 40` bits → 1（弱）
 * - `< 60` bits → 2（一般）
 * - `< 80` bits → 3（强）
 * - `≥ 80` bits → 4（很强）
 *
 * 参考量级（生成器场景）：4 类全开池 ≈ 94 → 6 位 ≈ 39 bits（弱）、8 位 ≈ 52（一般）、
 * 10 位 ≈ 66（强）、13 位 ≈ 85（很强）；纯小写池 26 → 8 位 ≈ 38（弱）、20 位 ≈ 94（很强）。
 * 注意：熵估算假设字符均匀随机——对人类口令（词典词组合）会**高估**，本指示仅作
 * 提示不做强制（U6 裁定不变）。
 */

/** 可打印 ASCII 非字母数字符号数（'!'..'~' 扣除字母数字）——与生成器符号目录同源量级。 */
const PRINTABLE_SYMBOL_POOL = 33;

/** 强度分级：0（空）~ 4（很强）。 */
export function scoreOf(value: string): number {
  if (value.length === 0) {
    return 0;
  }
  let pool = 0;
  if (/[a-z]/.test(value)) pool += 26;
  if (/[A-Z]/.test(value)) pool += 26;
  if (/[0-9]/.test(value)) pool += 10;
  if (/[^A-Za-z0-9]/.test(value)) pool += PRINTABLE_SYMBOL_POOL;
  const bits = value.length * Math.log2(Math.max(pool, 2));
  if (bits < 40) return 1;
  if (bits < 60) return 2;
  if (bits < 80) return 3;
  return 4;
}
