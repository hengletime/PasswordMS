// 源码扫描的唯一抽取函数（S10）。
//
// 为什么单独一个文件：`ipc-contract.test.ts` 与 `autotype-guard.test.ts` 都要从源码里
// 抽取带捕获组的名称集合（命令名、`err.*` 字面量）。两处各写一份正则，第一次改动就漂——
// 一处宽一处窄，两条守门就会对同一件事给出不同答案。
//
// 放在非 `.test.ts` 文件里而不是从某个测试文件导出：测试文件顶层有 `describe`，
// 被另一个测试文件 import 会把前者的用例再注册一遍。

/**
 * 从文本中按捕获组（第 1 组）提取去重并排序后的名称集合。
 *
 * @param text    源码全文（多文件时先 `join('\n')`）
 * @param pattern 必须带全局标志 `g` 且第 1 个捕获组是目标名称
 */
export function extractMatches(text: string, pattern: RegExp): string[] {
  const out = new Set<string>();
  for (const match of text.matchAll(pattern)) {
    out.add(match[1]);
  }
  return [...out].sort();
}
