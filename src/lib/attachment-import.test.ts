// 附件导入入口（D-59）的行为级测试。
// 缺陷背景：后端 import_attachment 与 AttachmentView 详情渲染全就绪，但前端零入口、
// api.keydocs.importAttachment 全项目零调用——附件功能从上线起不可达。
// 本文件证明入口流程的四条分支（守卫 / 取消 / 成功 / 失败）：
//   1. 取消或空路径 → importAttachment **零调用**、无 toast；
//   2. 成功 → IPC 以（源路径, 文件名标题）调用、success toast、返回新条目；
//   3. 失败（含超限）→ error toast（toAppError 统一映射）、不返回条目；
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

/** 伪造的新条目（仅含断言用字段）。 */
function fakeItem(id: string): ItemDto {
  return {
    item_id: id,
    kind: {
      type: 'attachment',
      title: 'report.pdf',
      original_filename: 'report.pdf',
      mime_type: 'application/pdf',
      size_bytes: 1024,
      blob_ref: id,
      blob_size_bytes: 2048,
    },
    folder_id: null,
    favorite: false,
    created_at: 0,
    updated_at: 0,
    rev: 1,
    modified_by: 'qa-device',
  } as unknown as ItemDto;
}

/** 构造一套可观测替身依赖；默认 pickFile 返回一个 Windows 路径。 */
function makeDeps(overrides: Partial<AttachmentImportDeps> = {}): AttachmentImportDeps & {
  importAttachment: ReturnType<typeof vi.fn>;
  toast: ReturnType<typeof vi.fn>;
} {
  // importAttachment/toast 必须从 overrides 中**取出**后再注入（不能被后面的简写覆盖回去）。
  const { importAttachment: importAttachmentOverride, ...rest } = overrides;
  const importAttachment =
    importAttachmentOverride ??
    vi.fn(async (_sourcePath: string, _title: string): Promise<ItemDto> => fakeItem('new-1'));
  const toast = vi.fn<(kind: 'info' | 'success' | 'error', messageKey: string) => void>();
  const deps: AttachmentImportDeps = {
    pickFile: async (): Promise<string | null> => 'C:\\Downloads\\report.pdf',
    importAttachment,
    toast: toast as unknown as AttachmentImportDeps['toast'],
    ...rest,
  };
  return { ...deps, importAttachment, toast } as AttachmentImportDeps & {
    importAttachment: ReturnType<typeof vi.fn>;
    toast: ReturnType<typeof vi.fn>;
  };
}

describe('runAttachmentImport —— D-59 附件导入入口', () => {
  it('取消选择：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ pickFile: async (): Promise<string | null> => null });

    const result = await runAttachmentImport(deps);

    expect(deps.importAttachment).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, item: null });
  });

  it('空路径守卫：不发 IPC、无 toast（流程接错则此用例变红）', async () => {
    const deps = makeDeps({ pickFile: async (): Promise<string | null> => '' });

    const result = await runAttachmentImport(deps);

    expect(deps.importAttachment).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, item: null });
  });

  it('成功：IPC 以（源路径, 文件名标题）调用；success toast；返回新条目', async () => {
    const deps = makeDeps();

    const result = await runAttachmentImport(deps);

    expect(deps.importAttachment).toHaveBeenCalledTimes(1);
    expect(deps.importAttachment).toHaveBeenCalledWith('C:\\Downloads\\report.pdf', 'report.pdf');
    expect(deps.toast).toHaveBeenCalledWith('success', ATTACHMENT_OK_KEY);
    expect(result.invoked).toBe(true);
    expect(result.item?.item_id).toBe('new-1');
  });

  it('失败（超限等）：error toast（统一错误码映射）、item 为 null', async () => {
    const deps = makeDeps({
      importAttachment: vi.fn(
        async (_sourcePath: string, _title: string): Promise<ItemDto> => {
          throw new Error('attachment too large');
        },
      ),
    });

    const result = await runAttachmentImport(deps);

    expect(deps.importAttachment).toHaveBeenCalledTimes(1);
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
