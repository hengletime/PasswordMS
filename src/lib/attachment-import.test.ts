// 附件添加入口（D-59 / D-60）的行为级测试。
// D-59 缺陷背景：后端就绪但前端零调用、附件功能不可达——本文件证明入口流程的
// 四条分支（守卫 / 取消 / 成功 / 失败）。
// D-60 改造：附件为父条目内嵌 `attachments` 成员，IPC 改 `addAttachment(itemId, …)`
// 且返回**父条目**（rev+1）；替身签名与断言随之更新。
//   1. 取消或空路径 → addAttachment **零调用**、无 toast；
//   2. 成功 → IPC 以（父条目 id, 源路径, 文件名标题）调用、success toast、返回父条目；
//   3. 失败（超限/上限/超长/容量）→ error toast（toAppError 统一映射）、不返回条目；
//   4. attachmentTitleFromPath 的分隔符兼容（Windows / POSIX / 无分隔符）。
// 流程若被误接断（如把 pickFile 结果直接丢弃），用例 2/3 的参数断言即红。

import { describe, expect, it, vi } from 'vitest';

import {
  ATTACHMENT_OK_KEY,
  attachmentTitleFromPath,
  runAttachmentImport,
  type AttachmentImportDeps,
} from './attachment-import';
import type { ItemDto } from '../types';

/** 伪造的父条目（仅含断言用字段；D-60：attachments 内嵌于父条目）。 */
function fakeParent(parentId: string): ItemDto {
  return {
    item_id: parentId,
    kind: { type: 'secure_note', title: 'work notes', body: '' },
    attachments: [
      {
        attachment_id: 'att-1',
        filename: 'report.pdf',
        mime_type: 'application/pdf',
        size_bytes: 1024,
        blob_size_bytes: 2048,
      },
    ],
    folder_id: null,
    favorite: false,
    created_at: 0,
    updated_at: 0,
    rev: 2,
    modified_by: 'qa-device',
  };
}

/** 构造一套可观测替身依赖；默认 pickFile 返回一个 Windows 路径。 */
function makeDeps(overrides: Partial<AttachmentImportDeps> = {}): AttachmentImportDeps & {
  addAttachment: ReturnType<typeof vi.fn>;
  toast: ReturnType<typeof vi.fn>;
} {
  // addAttachment/toast 必须从 overrides 中**取出**后再注入（不能被后面的简写覆盖回去）。
  const { addAttachment: addAttachmentOverride, ...rest } = overrides;
  const addAttachment =
    addAttachmentOverride ??
    vi.fn(async (itemId: string, _sourcePath: string, _title: string): Promise<ItemDto> =>
      fakeParent(itemId),
    );
  const toast = vi.fn<(kind: 'info' | 'success' | 'error', messageKey: string) => void>();
  const deps: AttachmentImportDeps = {
    pickFile: async (): Promise<string | null> => 'C:\\Downloads\\report.pdf',
    addAttachment,
    toast: toast as unknown as AttachmentImportDeps['toast'],
    ...rest,
  };
  return { ...deps, addAttachment, toast } as AttachmentImportDeps & {
    addAttachment: ReturnType<typeof vi.fn>;
    toast: ReturnType<typeof vi.fn>;
  };
}

describe('runAttachmentImport —— D-59/D-60 附件添加入口', () => {
  it('取消选择：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ pickFile: async (): Promise<string | null> => null });

    const result = await runAttachmentImport(deps, 'parent-1');

    expect(deps.addAttachment).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, item: null });
  });

  it('空路径守卫：不发 IPC、无 toast（流程接错则此用例变红）', async () => {
    const deps = makeDeps({ pickFile: async (): Promise<string | null> => '' });

    const result = await runAttachmentImport(deps, 'parent-1');

    expect(deps.addAttachment).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, item: null });
  });

  it('成功：IPC 以（父条目 id, 源路径, 文件名标题）调用；success toast；返回父条目', async () => {
    const deps = makeDeps();

    const result = await runAttachmentImport(deps, 'parent-1');

    expect(deps.addAttachment).toHaveBeenCalledTimes(1);
    expect(deps.addAttachment).toHaveBeenCalledWith('parent-1', 'C:\\Downloads\\report.pdf', 'report.pdf');
    expect(deps.toast).toHaveBeenCalledWith('success', ATTACHMENT_OK_KEY);
    expect(result.invoked).toBe(true);
    expect(result.item?.item_id).toBe('parent-1');
    expect(result.item?.attachments).toHaveLength(1);
  });

  it('失败（超限等）：error toast（统一错误码映射）、不返回条目', async () => {
    const deps = makeDeps({
      addAttachment: vi.fn(
        async (_itemId: string, _sourcePath: string, _title: string): Promise<ItemDto> => {
          throw new Error('attachment limit');
        },
      ),
    });

    const result = await runAttachmentImport(deps, 'parent-1');

    expect(deps.addAttachment).toHaveBeenCalledTimes(1);
    expect(deps.toast).toHaveBeenCalledWith('error', 'err.internal');
    expect(result).toEqual({ invoked: true, item: null });
  });
});

describe('attachmentTitleFromPath —— 默认标题提取（与 core 层回退语义一致：含扩展名）', () => {
  it('Windows 路径取最后一段', () => {
    expect(attachmentTitleFromPath('C:\\Downloads\\report.pdf')).toBe('report.pdf');
  });

  it('POSIX 路径取最后一段', () => {
    expect(attachmentTitleFromPath('/home/qa/keyfile.kdbx')).toBe('keyfile.kdbx');
  });

  it('无分隔符时原样返回', () => {
    expect(attachmentTitleFromPath('notes.txt')).toBe('notes.txt');
  });
});
