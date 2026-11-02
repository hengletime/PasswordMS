// D-172 导入原子性 + 失败可重试（Kimi 审计 F-07 / F-08）的接线守卫。
//
// 这一轮的修法全部落在「顺序」和「谁调谁」上，而这四种错法**编译得过、行为也像对的**：
//
// 1. **预览先删还是后删**：`import_apply` 里 `pending_imports.remove` 若回到
//    `lock_session` 之前，库已锁定 / 写盘失败时预览照样被吃掉——F-08 原样复现。
// 2. **失败要不要归还**：只靠 `remove` 不 `insert` 回去，等于「失败即消费」。
// 3. **落库是不是一次**：`import::apply` 若又长回 `put_item_direct` 逐条写，
//    第 700 条失败就会留下前 699 条（F-07 原样复现）。
// 4. **回滚是否真被调用**：`put_records_atomic` 里 `append_frames` 之后若漏了
//    `rebuild_index` 回读校验 / 漏了 `truncate_to` 回滚，写坏与写一半都发现不了。
//
// 行为正确性由 core `tests/d172_import_atomic.rs`（5 例，含 Windows 只读库文件真实写盘
// 失败）负责；本文件只钉接线。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../src-tauri/src/commands/import_cmd.rs',
    '../../crates/passwordms-core/src/import/mod.rs',
    '../../crates/passwordms-core/src/store/repository.rs',
    '../../crates/passwordms-core/src/store/container.rs',
    '../components/ImportWizard.svelte',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  const hit = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1];
  if (hit === undefined || hit.length === 0) {
    throw new Error(`读不到源码：${suffix}（glob 空集会静默通过，故显式抛错）`);
  }
  return hit;
}

function fnBody(source: string, signature: string): string {
  const at = source.indexOf(signature);
  if (at < 0) {
    throw new Error(`找不到 ${signature}`);
  }
  let i = source.indexOf('{', at);
  if (i < 0) {
    throw new Error(`${signature} 之后没有函数体`);
  }
  let depth = 0;
  for (; i < source.length; i += 1) {
    if (source[i] === '{') {
      depth += 1;
    } else if (source[i] === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(at, i + 1);
      }
    }
  }
  throw new Error(`${signature} 的函数体没有闭合`);
}

function indexOfOrThrow(haystack: string, needle: string, label: string): number {
  const at = haystack.indexOf(needle);
  if (at < 0) {
    throw new Error(`${label}：找不到 ${needle}（判据本身就失效了，不是实现对不对的问题）`);
  }
  return at;
}

const importCmd = src('commands/import_cmd.rs');
const importMod = src('import/mod.rs');
const repository = src('repository.rs');
const container = src('container.rs');
const wizard = src('ImportWizard.svelte');

const applyCmd = fnBody(importCmd, 'pub fn import_apply(');
const applyCore = fnBody(importMod, 'pub fn apply(');
const putAtomic = fnBody(repository, 'pub fn put_records_atomic(');
const appendFrames = fnBody(container, 'pub fn append_frames(');
const truncateTo = fnBody(container, 'pub fn truncate_to(');

describe('F-08：预览的消费时机是「成功之后」', () => {
  it('先拿会话锁，再取预览', () => {
    const lock = indexOfOrThrow(applyCmd, 'state.lock_session()', '会话锁');
    const take = indexOfOrThrow(applyCmd, '.remove(&preview_token)', '取预览');
    expect(lock).toBeLessThan(take);
  });

  it('取预览发生在 repo 可用之后（锁失败时预览原封不动）', () => {
    const repo = indexOfOrThrow(applyCmd, 'session.repo_mut()', 'repo_mut');
    const take = indexOfOrThrow(applyCmd, '.remove(&preview_token)', '取预览');
    expect(repo).toBeLessThan(take);
  });

  it('失败必须把预览放回缓存，成功才消费', () => {
    expect(applyCmd).toContain('cache.insert(preview_token, preview)');
    // 成功分支不得归还（否则同一预览可被重复提交，第二次全是重复跳过，结果看着像成功）。
    // 判据要钉「在 Err 分支里」：只比较「在 Ok 之后」的话，把归还写进 Ok 分支照样通过。
    const okBranch = indexOfOrThrow(applyCmd, 'Ok(result) =>', '成功分支');
    const errBranch = indexOfOrThrow(applyCmd, 'Err(err) =>', '失败分支');
    const putBack = indexOfOrThrow(applyCmd, 'cache.insert(preview_token', '归还');
    expect(okBranch).toBeLessThan(errBranch);
    expect(putBack).toBeGreaterThan(errBranch);
  });
});

describe('F-07：导入全成或全无', () => {
  it('core apply 只调一次批量写入，不再逐条落库', () => {
    expect(applyCore).toContain('repo.put_records_atomic(&folders_to_add, &items_to_add)');
    expect(applyCore).not.toContain('put_item_direct');
    expect(applyCore).not.toContain('put_folder_direct');
    // 计数由批量写入内部一起落盘；外面再调一次等于「记录已写、计数也写了，但失败不回滚」。
    expect(applyCore).not.toContain('persist_counts');
  });

  it('规划阶段只收集，不写库', () => {
    expect(applyCore).toContain('let mut folders_to_add: Vec<Folder> = Vec::new();');
    expect(applyCore).toContain('let mut items_to_add: Vec<Item> = Vec::new();');
    // 唯一的写调用必须排在两个收集循环之后。
    const lastPush = applyCore.lastIndexOf('items_to_add.push(item);');
    const write = indexOfOrThrow(applyCore, 'put_records_atomic(', '批量写入');
    expect(lastPush).toBeGreaterThan(-1);
    expect(write).toBeGreaterThan(lastPush);
  });

  it('批量写入先校验编码、再追加、再回读、最后落计数', () => {
    const append = indexOfOrThrow(putAtomic, 'append_frames(&frames)', '追加');
    const rescan = indexOfOrThrow(putAtomic, 'rebuild_index()', '回读校验');
    const persist = indexOfOrThrow(putAtomic, 'persist_header()', '计数落盘');
    expect(append).toBeLessThan(rescan);
    expect(rescan).toBeLessThan(persist);
    // 空批次不碰磁盘（全重复的第二次导入不该改文件）。
    expect(putAtomic).toContain('if folders.is_empty() && items.is_empty()');
  });

  it('两条失败路径都必须截回写前长度', () => {
    // 回读失败 + 计数失败，各一次。
    const hits = putAtomic.split('truncate_to(mark)').length - 1;
    expect(hits).toBe(2);
  });

  it('帧在打开文件之前全部编码（编码失败时磁盘一字未动）', () => {
    const encode = indexOfOrThrow(appendFrames, 'frame.encode()?', '帧编码');
    const open = indexOfOrThrow(appendFrames, 'OpenOptions::new().append(true)', '打开文件');
    expect(encode).toBeLessThan(open);
  });

  it('截断只往小里截（set_len 延长会把记录区变成零字节）', () => {
    expect(truncateTo).toContain('if self.file_size()? <= len');
    expect(truncateTo).toContain('file.set_len(len)?;');
    expect(truncateTo).toContain('sync_all()');
  });

describe('界面：失败要说清「一条都没写入」', () => {
  const applyFn = fnBody(wizard, 'async function apply(');

  it('失败分支给出可重试的说明，并标成错误色', () => {
    expect(applyFn).toContain("ui.t('import.nothing_written')");
    expect(applyFn).toContain('resultIsError = true;');
    // 成功分支必须把错误态清掉，否则重试成功后会留着一条红字。
    expect(applyFn).toContain('resultIsError = false;');
  });

  it('结果文本按态着色，失败不能用成功色', () => {
    expect(wizard).toContain('class:is-error={resultIsError}');
    expect(wizard).toContain('.imp-result.is-error');
  });

  it('重试前清掉上一次的结果文本', () => {
    const clear = indexOfOrThrow(applyFn, "resultText = '';", '清空结果');
    const call = indexOfOrThrow(applyFn, 'api.importer.apply(', '执行导入');
    expect(clear).toBeLessThan(call);
  });
});
});
