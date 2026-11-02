<script lang="ts">
  // 条目编辑器（PRD F-04 / F-13）：三种 kind 的表单 + 自定义字段增删 + URL 校验 + 生成器。
  // 设计规范 §5.4：全部换用 .pms-field / .pms-input-group；自定义字段区用 .pms-card 内嵌；底部操作条 sticky。
  import Icon from './common/Icon.svelte';
  import PasswordGeneratorModal from './PasswordGeneratorModal.svelte';
  import { MAX_NOTE_BYTES } from '../lib/constants';
  import { isSafeUrl } from '../lib/url-safety';
  import { toAppError, ErrorCode } from '../lib/errors';
  import { currentTotp, normalizeTotpInput } from '../lib/totp';
  import { parseOtpauth, rejectMessage } from '../lib/otpauth';
  import { decodeQrFromBlob } from '../lib/qr';
  import { startCameraScan, type CameraScan } from '../lib/qr-camera';
  import {
    clipboardImageSupported,
    readClipboardImage,
    type ClipboardImage,
  } from '../lib/clipboard-image';
  import {
    captureScreen,
    cropShotToBlob,
    disposeShot,
    type CropRect,
    type ScreenShot,
  } from '../lib/screen-capture';
  import ScreenCrop from './ScreenCrop.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { tick } from 'svelte';

  /**
   * 藏窗口后等多久再抓屏。
   *
   * 窗口从"被要求隐藏"到"真的从合成器上消失"之间有动画与合成延迟；藏完立刻抓，
   * 抓到的是还没消失的那一帧 —— 那一帧里正好是本应用自己的界面。180ms 在本机实测
   * 足够，且用户察觉不到（本来就要等抓屏）。
   */
  const HIDE_SETTLE_MS = 180;
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { CustomField, ItemDraftDto, ItemDto } from '../types';

  interface Props {
    item?: ItemDto | null;
    kind: 'login' | 'secure_note';
    ondone: () => void;
    oncancel: () => void;
  }
  const { item, kind, ondone, oncancel }: Props = $props();

  // 打开编辑器时对条目做一次性快照（表单为本地可编辑副本，父组件后续变化不覆盖）。
  // svelte-ignore state_referenced_locally
  const login = item?.kind.type === 'login' ? item.kind : null;
  // svelte-ignore state_referenced_locally
  const note = item?.kind.type === 'secure_note' ? item.kind : null;

  let title = $state(login?.title ?? note?.title ?? '');
  let url = $state(login?.url ?? '');
  let username = $state(login?.username ?? '');
  let password = $state(login?.password ?? '');
  let notes = $state(login?.notes ?? '');
  let totp = $state(login?.totp_secret ?? '');
  let body = $state(note?.body ?? '');
  // D-81：新建时**预填当前选中分组**。原来恒为 null，条目会存进根目录；
  // 若此刻正选中某分组（列表按它过滤），保存后条目立刻「消失」——编辑器关闭、
  // 列表没有、详情区没有，用户以为丢了（实际在根目录）。编辑已有条目路径不变。
  // 分组下拉的 options 恒为全部分组，预填值必合法。
  // svelte-ignore state_referenced_locally
  let folderId = $state<string | null>(item?.folder_id ?? vault.selectedFolderId);
  // svelte-ignore state_referenced_locally
  let favorite = $state(item?.favorite ?? false);
  let fields = $state<CustomField[]>(
    login ? login.custom_fields.map((f): CustomField => ({ ...f })) : [],
  );
  let showPassword = $state(false);
  let showGenerator = $state(false);
  // D-97（A）：TOTP 密钥入库前校验失败 → 行内错误态（对齐 urlUnsafe 模式）。
  let totpInvalid = $state(false);
  // D-97：校验失败聚焦用（bind:this）。
  let totpInput = $state<HTMLInputElement | null>(null);

  // ===== D-162：两步验证二维码（扫码 / 从图片导入）=====
  //
  // 为什么要有这个：绝大多数网站的两步验证设置页给的是**二维码**，旁边才有一行
  // 「手动输入密钥」的小字。手抄 Base32 又慢又容易错（0/O、1/I 是经典错源），
  // 而既有实现只支持「粘贴 otpauth 链接」——那要求用户先把二维码转成文本，绕了一圈。
  //
  // 两个入口都保留，因为二维码常常就显示在**用户自己这块屏幕上**（网站设置页在浏览器里、
  // App 在手机上）：这时摄像头拍自己的屏幕是最难的对焦条件，「从图片」才是主力路径。
  let scanOpen = $state(false);
  let scanVideo = $state<HTMLVideoElement | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);
  /** 摄像头打不开（权限被拒 / 无设备）→ 收起预览，只留提示与图片入口。 */
  let scanCameraFailed = $state(false);
  /**
   * TOTP 字段的行内提示（i18n 键 + 参数）；`null` 表示无提示。
   *
   * 扫码与粘贴**共用**这一个状态：两个入口走的是同一条参数判定（见 `parseOtpauth`），
   * 提示也就该落在同一个位置 —— 分两套状态迟早出现「扫码说不行、粘贴没反应」。
   */
  let totpHint = $state<{ key: string; params?: Record<string, string> } | null>(null);
  // 刻意不用 $state：它是设备句柄，不是渲染数据；若做成响应式，下面的 effect 会因
  // 「赋值 scan」而重跑，等于每启动一次摄像头就重启一次。
  let scan: CameraScan | null = null;
  /** 截图流程进行中：按钮禁用（藏窗口 + 抓屏 + 还原，几百毫秒的忙期）。 */
  let shotPending = $state(false);
  /** 读剪贴板 + 解码的进行中标记（防连点；这条路径有几十毫秒的忙期）。 */
  let pastePending = $state(false);
  /**
   * 抓下来的屏幕快照，非 `null` 时框选层展开。
   *
   * 它是**冻结**的画面：抓下来之后屏幕怎么变都与它无关，所以窗口可以立刻还原，
   * 用户框到的永远是抓取那一刻的内容。这正是 1.5.20 缺的东西 ——
   * 那次把窗口还给了用户，画面却是活的，窗口自己被框了进去。
   */
  let cropShot = $state<ScreenShot | null>(null);
  /**
   * 框选层里显示的提示文案。
   *
   * 复用 `totpHint` 而不是另开一份：字段下方与框选层说的是同一件事，
   * 分两份状态必然出现「框选层说没识别到、字段下方还挂着上一条」的错位。
   */
  const cropHint = $derived(totpHint === null ? null : ui.t(totpHint.key, totpHint.params));

  /**
   * 消费一张码解出的文本。
   *
   * @returns 是否已消费（`true` 停止扫描）。**非 `otpauth://` 的一律返回 `false`**：
   * 摄像头视野里常有别的二维码（商品条码、普通网址），把每一条都当错误会让人以为坏了。
   */
  function handleScannedText(raw: string): boolean {
    const trimmed = raw.trim();
    // 只判前缀就用 `startsWith`，不写正则字面量：这里没有任何模式匹配需求，
    // 而正则里的转义斜杠在多层文本传递中最容易被吃掉（本轮实际踩过一次）。
    if (!trimmed.toLowerCase().startsWith('otpauth://')) {
      return false;
    }
    const parsed = parseOtpauth(trimmed);
    if (parsed.kind === 'rejected') {
      // **拒绝时绝不写 `totp`**：存量计算固定 6 位 / 30 秒 / SHA1，把 digits=8 的密钥存进去
      // 会得到一个「存得进去、算出来必然对不上」的条目 —— 比当场报错难查得多。
      totpHint = rejectMessage(parsed);
      return true;
    }
    totp = parsed.secret;
    totpInvalid = false;
    totpHint = null;
    // 标题只在**空**的时候补：用户已经写了一半就说明他要自己的命名，覆盖比不填更烦人。
    if (title.trim() === '') {
      title = parsed.issuer ?? parsed.account ?? '';
    }
    closeScan();
    return true;
  }

  function closeScan(): void {
    scan?.stop();
    scan = null;
    scanOpen = false;
    // 刻意不清 `totpHint`：拒绝原因要留在字段旁边，用户才有机会读懂它。
    // 「关掉面板」不等于「问题解决了」。
  }

  function toggleScan(): void {
    if (scanOpen) {
      closeScan();
      return;
    }
    scanOpen = true;
    totpHint = null;
    scanCameraFailed = false;
  }

  /**
   * 解码一张图并把结果落到表单。
   *
   * @returns 是否**成功填入**。框选层用它决定要不要收起：填成功了就退，
   * 没识别到就留在原地让用户再框一次（退出重来要多付一次整屏抓取）。
   */
  async function ingestImage(file: Blob): Promise<boolean> {
    const text = await decodeQrFromBlob(file);
    if (text === null) {
      totpHint = { key: 'editor.scan_no_qr' };
      return false;
    }
    if (!handleScannedText(text)) {
      // 解出了码但不是两步验证码：明确说出来，别表现成「点了没反应」。
      totpHint = { key: 'editor.scan_not_otpauth' };
      return false;
    }
    return true;
  }

  async function pickImage(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    // 立刻清空：否则连选同一个文件第二次不会触发 change（浏览器不会发事件）。
    input.value = '';
    if (file === null) {
      return;
    }
    await ingestImage(file);
  }

  /** 把一次剪贴板读图的结果落到界面上 —— 每种结局都要有话说，不许「点了没反应」。 */
  async function consumeClipboard(result: ClipboardImage): Promise<void> {
    switch (result.kind) {
      case 'image':
        await ingestImage(result.blob);
        return;
      case 'empty':
        totpHint = { key: 'editor.paste_empty' };
        return;
      case 'unsupported':
        totpHint = { key: 'editor.paste_unsupported' };
        return;
      default:
        totpHint = { key: 'editor.paste_failed' };
    }
  }

  /** 「粘贴截图」：直接读剪贴板里的位图（不经过任何键盘事件）。 */
  async function pasteImage(): Promise<void> {
    if (pastePending) {
      return;
    }
    pastePending = true;
    totpHint = null;
    try {
      await consumeClipboard(await readClipboardImage());
    } finally {
      pastePending = false;
    }
  }

  /**
   * 「截图」：藏窗口 → **自己抓屏** → 还原窗口 → 在冻结的画面上框选。
   *
   * 上一版（1.5.20）走的是系统截图工具 `ms-screenclip:`，并且相信「覆盖层一出现
   * 屏幕就冻结了」，于是 1.2 秒后把窗口还给了用户。实测：**覆盖层是活画面**，
   * 窗口一回来就出现在框里 —— 用户还没开始框，图里已经有 PasswordMS 了。
   *
   * 现在把顺序倒过来：**先抓，再还原**。抓下来的像素不会变，用户框哪儿就是哪儿，
   * 窗口什么时候回来都无所谓。这也顺带解决了「用户按 Esc 取消后窗口回不来」——
   * 框选层是我们自己的，取消立刻生效。
   *
   * 窗口藏失败（缺权限 / 非 Tauri）不阻断流程：抓到的图里会包含本应用窗口，
   * 但至少功能还能用，用户框的时候能看见该怎么绕开。
   */
  async function startScreenshot(): Promise<void> {
    if (shotPending) {
      return;
    }
    totpHint = null;
    shotPending = true;
    // 句柄本身也可能拿不到（非 Tauri 运行时）。放在 try 里取，否则这一步抛错会把
    // `shotPending` 永远卡在 true —— 按钮从此点不动，而且没有任何提示。
    let win: ReturnType<typeof getCurrentWindow> | null = null;
    let hidden = false;
    try {
      try {
        win = getCurrentWindow();
        await win.hide();
        hidden = true;
      } catch {
        // 权限缺失 / 非 Tauri 运行时：不藏也要把截图做下去。
      }
      if (hidden) {
        // 等窗口真的从合成器上消失。藏完立刻抓，抓到的是还没消失的那一帧 ——
        // 这一帧里正好是本应用自己的界面。
        await new Promise((resolve) => setTimeout(resolve, HIDE_SETTLE_MS));
      }
      const result = await captureScreen();
      if (result.kind !== 'ok') {
        totpHint = {
          key: result.kind === 'unsupported' ? 'editor.shot_unavailable' : 'editor.shot_failed',
        };
        return;
      }
      disposeShot(cropShot);
      cropShot = result.shot;
      // 等 Svelte 把框选层渲染出来再还原窗口：否则窗口先出现、编辑器闪一下，
      // 用户会以为「点了截图，什么都没发生」。
      await tick();
    } finally {
      if (hidden && win !== null) {
        try {
          await win.show();
          await win.setFocus();
        } catch {
          // 恢复失败无处可退：窗口还在任务栏，用户至少能再点一次。
        }
      }
      shotPending = false;
    }
  }

  /** 收掉框选层并把快照的像素内存还回去（4K 一张 33MB，不能等 GC）。 */
  function closeCrop(): void {
    disposeShot(cropShot);
    cropShot = null;
  }

  /**
   * 框选完成 → 裁出那一块 → 解码。
   *
   * 失败（这块里没有码 / 不是 otpauth）时**不收框选层**：提示留在画面上，
   * 用户可以直接再框一次。收掉的话，重来要再藏一次窗口再抓一次屏。
   */
  async function onCropPick(rect: CropRect): Promise<void> {
    const shot = cropShot;
    if (shot === null) {
      return;
    }
    totpHint = null;
    const blob = await cropShotToBlob(shot, rect);
    if (blob === null) {
      totpHint = { key: 'editor.paste_failed' };
      return;
    }
    if (await ingestImage(blob)) {
      closeCrop();
    }
  }

  // 卸载时把快照的像素还回去（面板那条 effect 只管摄像头自己的生命周期）。
  $effect((): (() => void) => {
    return () => {
      disposeShot(cropShot);
      cropShot = null;
    };
  });

  // 粘贴监听**不挂在扫码面板上**（D-163 修的根因）。
  //
  // 原实现是「面板打开时才监听」，想法是"别拦用户往别处粘图"，实际效果却是：
  // 用户看到二维码的第一反应是当场截图、当场 Ctrl+V，不会先去点开一个面板 ——
  // 于是功能明明在，却表现为「截图在剪贴板里，按了没反应」。功能被一层
  // 没人知道的开关挡着，和没做是一样的。
  //
  /**
   * 读粘贴内容里的纯文本。
   *
   * 包 `try`：`getData` 在个别宿主上会因剪贴板权限抛异常，而这里一抛，
   * 整个 paste 处理器就中断了 —— 用户看到的是「粘贴彻底没反应」，
   * 比「没识别到图片」难查得多。读不到就当作没有文本，交给后面的兜底。
   */
  function textOf(data: DataTransfer | null): string {
    try {
      return data?.getData('text/plain') ?? '';
    } catch {
      return '';
    }
  }

  // 现在改成编辑器挂载期间全程监听，靠**「这次粘贴到底有没有内容」**来收窄范围：
  // 没有内容就一个字节都不碰（用户可能就想粘文本到密码框）。这比"某个面板是否展开"
  // 更贴近真正要保护的东西。
  //
  // D-164：只看 `items` 里有没有 image/* 还不够。桌面 WebView2 经常出现
  // 「paste 事件派发了，但 DataTransfer 里既没有图片文件也没有文本」——
  // 宿主根本没把剪贴板位图转换过来。此时唯一的路是原生 `readImage()`，
  // 也就是「粘贴截图」按钮那条已被真机验证可用的路。三路判定，缺一路就表现为
  // 「截图明明在剪贴板里，Ctrl+V 却毫无反应」：
  //   ① 有 image/* 文件 → 直接解码；
  //   ② 有文本         → 放行，让浏览器正常粘贴（可能就是在粘 otpauth:// 整串）；
  //   ③ 两者都没有     → preventDefault + 原生读图。
  $effect((): (() => void) => {
    let sawPaste = false;
    let fallbackTimer = 0;

    const onPaste = (event: ClipboardEvent): void => {
      sawPaste = true;
      const data = event.clipboardData;
      const items = data?.items;
      // 有没有「文件」项，和有没有「图片」项是两件事：
      // 从资源管理器复制一个 .txt 过来时 items 里有 file 但 type 不是 image，
      // 那不是我们该接的东西 —— 放行，别拦下来再弹一句「剪贴板里没有图片」。
      let hasAnyFile = false;
      if (items !== undefined) {
        for (const item of Array.from(items)) {
          if (item.kind !== 'file') {
            continue;
          }
          hasAnyFile = true;
          if (!item.type.startsWith('image/')) {
            continue;
          }
          const file = item.getAsFile();
          if (file !== null) {
            event.preventDefault();
            void ingestImage(file);
            return;
          }
        }
      }
      if (hasAnyFile) {
        return;
      }
      if (textOf(data) !== '') {
        // 正常的文本粘贴：一个字节都别碰。
        return;
      }
      // ③ 宿主什么都没交给我们 —— 但剪贴板里很可能正躺着一张截图。
      event.preventDefault();
      void pasteImage();
    };

    // 兜底：有的宿主根本不派发 paste 事件。此时浏览器自己也没粘任何东西
    // （否则事件会来），所以这里读原生剪贴板不会抢走用户的文本粘贴。
    const onKeydown = (event: KeyboardEvent): void => {
      const combo = (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey;
      if (!combo || event.key.toLowerCase() !== 'v') {
        return;
      }
      sawPaste = false;
      window.clearTimeout(fallbackTimer);
      fallbackTimer = window.setTimeout(() => {
        if (!sawPaste) {
          void pasteImage();
        }
      }, 200);
    };

    window.addEventListener('paste', onPaste);
    window.addEventListener('keydown', onKeydown);
    return (): void => {
      window.clearTimeout(fallbackTimer);
      window.removeEventListener('paste', onPaste);
      window.removeEventListener('keydown', onKeydown);
    };
  });

  // 面板展开时开摄像头，收起 / 卸载时释放。
  $effect((): (() => void) => {
    if (!scanOpen || scanVideo === null) {
      return () => {};
    }
    // `cancelled` 而不是读 `scanOpen`：授权弹窗期间用户可能已经关了面板甚至离开编辑器，
    // 那时 stream 才到手 —— 必须当场还回去，否则摄像头灯一直亮着（D-162 修的既有泄漏）。
    let cancelled = false;
    void startCameraScan({
      video: scanVideo,
      onText: handleScannedText,
      onError: (kind) => {
        scanCameraFailed = true;
        totpHint = { key: kind === 'other' ? 'err.internal' : 'editor.scan_camera_denied' };
      },
    }).then((handle) => {
      if (cancelled) {
        handle.stop();
        return;
      }
      scan = handle;
    });
    return () => {
      cancelled = true;
      scan?.stop();
      scan = null;
    };
  });

  let noteBytes = $derived(new Blob([body]).size);
  let bodyTooLong = $derived(kind === 'secure_note' && noteBytes > MAX_NOTE_BYTES);
  let urlUnsafe = $derived(kind === 'login' && url.trim().length > 0 && !isSafeUrl(url));
  let canSave = $derived(
    title.trim().length > 0 && !bodyTooLong && !urlUnsafe && !vault.busy,
  );

  function addField(): void {
    fields = [...fields, { key: '', value: '', hidden: false }];
  }

  function removeField(index: number): void {
    fields = fields.filter((_value, i): boolean => i !== index);
  }

  async function save(): Promise<void> {
    if (!canSave) {
      return;
    }
    // D-97（A+C）：TOTP 入库前真校验。otpauth:// 链接先提取 secret（C），
    // 再调 Rust 侧 totp_now 试算（等价 base32 真解码，含 0/1/8/9 等非法字符必失败）。
    // 仅 ERR_VALIDATION 拦截保存并聚焦字段；环境类错误放行原流程（不因 IPC 故障卡死编辑器）。
    let totpSecret: string | null = null;
    if (kind === 'login' && totp.trim().length > 0) {
      const raw = totp.trim();
      // D-162：粘贴 otpauth 整串时先过**与扫码完全同一套**参数判定。
      //
      // 只抽 secret（D-97 的做法）会放过 digits=8 / period=60 / SHA256 的码：
      // 存进去以后本地算出来的 6 位码在网站上必然被拒，而用户没有任何线索知道为什么。
      // 扫码路径已经明确拒绝 —— 若粘贴路径还放行，就成了「同一个密钥，扫进来不行、
      // 粘进来能存」，比两处都不判更难解释。
      if (raw.toLowerCase().startsWith('otpauth://')) {
        const parsed = parseOtpauth(raw);
        if (parsed.kind === 'rejected') {
          totpHint = rejectMessage(parsed, 'paste');
          totpInput?.focus();
          return;
        }
        totpSecret = parsed.secret;
      } else {
        totpSecret = normalizeTotpInput(raw);
      }
      try {
        await currentTotp(totpSecret);
        totpInvalid = false;
        totpHint = null;
      } catch (err) {
        if (toAppError(err).code === ErrorCode.ERR_VALIDATION) {
          totpInvalid = true;
          totpInput?.focus();
          return;
        }
        totpInvalid = false;
      }
    } else if (kind === 'login') {
      totpInvalid = false;
    }
    let draft: ItemDraftDto;
    if (kind === 'login') {
      draft = {
        kind: {
          type: 'login',
          title: title.trim(),
          url: url.trim() || null,
          username: username.trim() || null,
          password: password.length > 0 ? password : null,
          notes: notes.trim() || null,
          totp_secret: totpSecret,
          custom_fields: fields.filter((f): boolean => f.key.trim().length > 0),
        },
        folder_id: folderId,
        favorite,
      };
    } else {
      draft = {
        kind: { type: 'secure_note', title: title.trim(), body },
        folder_id: folderId,
        favorite,
      };
    }
    const existing = item;
    const result =
      existing !== null && existing !== undefined && existing.kind.type === kind
        ? await vault.updateItem(existing.item_id, draft)
        : await vault.createItem(draft);
    if (result !== null) {
      ondone();
    }
  }
</script>

<form
  class="editor"
  onsubmit={(event: SubmitEvent): void => {
    event.preventDefault();
    void save();
  }}
>
  <!-- D-92/D-93：字段区独占滚动层，操作条固定贴底（不再 sticky）。
       根因①：桌面端 .pane 为 display:contents，form.editor 直接成为 .body(flex row) 的 item，
       旧版只有移动端媒体查询给了 flex:1 → 桌面宽度按内容收缩成窄列（笔记/登录同病）。
       根因②：sticky bottom:0 的吸附范围被包含块限制，最低贴到 .editor 内容盒底，
       其下 padding 空带露出、滚过的字段（分组下拉）从带里透出；内容不足一屏时
       工具条停在内容末尾，与容器底之间也是空白。改为 flex 固定底栏后两者在几何上不可能复现。 -->
  <div class="editor-scroll">
  <div class="pms-field">
    <label class="pms-label" for="ed-title">{ui.t('editor.field_title')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="ed-title" class="pms-input" bind:value={title} autofocus />
  </div>

  {#if kind === 'login'}
    <div class="pms-field">
      <label class="pms-label" for="ed-url">{ui.t('editor.field_url')}</label>
      <input id="ed-url" class="pms-input" bind:value={url} type="url" placeholder="https://" aria-invalid={urlUnsafe} />
      {#if urlUnsafe}
        <p class="pms-hint is-danger">{ui.t('err.unsafe_url')}</p>
      {/if}
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-username">{ui.t('editor.field_username')}</label>
      <input id="ed-username" class="pms-input" bind:value={username} autocomplete="off" />
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-password">{ui.t('editor.field_password')}</label>
      <div class="pms-input-group">
        <input
          id="ed-password"
          class="pms-input"
          bind:value={password}
          type={showPassword ? 'text' : 'password'}
          autocomplete="new-password"
        />
        <button
          class="pms-icon-btn"
          type="button"
          title={showPassword ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
          aria-label={showPassword ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
          onclick={() => (showPassword = !showPassword)}
        >
          <Icon name={showPassword ? 'eye' : 'eye_off'} size={16} />
        </button>
        <button
          class="pms-icon-btn"
          type="button"
          title={ui.t('gen.title')}
          aria-label={ui.t('gen.title')}
          onclick={() => (showGenerator = true)}
        >
          <Icon name="dice" size={16} />
        </button>
      </div>
      <PasswordGeneratorModal
        open={showGenerator}
        onclose={() => (showGenerator = false)}
        onuse={(value: string): void => {
          password = value;
          showGenerator = false;
        }}
      />
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-totp">{ui.t('editor.field_totp')}</label>
      <!-- D-97：支持直接粘贴 otpauth:// 链接（保存时自动提取 secret 并试算校验）。
           非法字符（如 0/1/8/9）在入库前被拦截并聚焦本字段，不再产生「反复弹输入无效」的坏存量。 -->
      <input
        id="ed-totp"
        class="pms-input"
        bind:value={totp}
        bind:this={totpInput}
        autocomplete="off"
        placeholder={ui.t('editor.totp_placeholder')}
        aria-invalid={totpInvalid}
      />

      <!-- D-162 / D-163：两步验证二维码的入口。
           手机：码在别的设备上 → 扫码；截图就在相册里 → 从图片。
           桌面：码几乎总在**这块屏幕自己**上，所以主力是「从图片 / 粘贴 / 截图」，
           摄像头排最后 —— 拿笔记本摄像头拍自己的屏幕，是最难对焦的活。
           「粘贴 / 截图」按平台能力给：Android 读不了剪贴板位图（插件明确不支持），
           给了按钮也是骗人的。 -->
      <div class="editor-scan-row">
        {#if clipboardImageSupported()}
          <button class="pms-btn is-ghost is-sm" type="button" onclick={() => fileInput?.click()}>
            <Icon name="file" size={14} />{ui.t('editor.totp_scan_image')}
          </button>
          <button
            class="pms-btn is-ghost is-sm"
            type="button"
            disabled={pastePending}
            onclick={() => void pasteImage()}
          >
            <Icon name="clipboard" size={14} />{ui.t('editor.totp_paste')}
          </button>
          <button
            class="pms-btn is-ghost is-sm"
            type="button"
            disabled={shotPending}
            onclick={() => void startScreenshot()}
          >
            <Icon name="crop" size={14} />{ui.t('editor.totp_screenshot')}
          </button>
          <button class="pms-btn is-ghost is-sm" type="button" onclick={toggleScan}>
            <Icon name="qr" size={14} />{ui.t('editor.totp_scan')}
          </button>
        {:else}
          <button class="pms-btn is-ghost is-sm" type="button" onclick={toggleScan}>
            <Icon name="qr" size={14} />{ui.t('editor.totp_scan')}
          </button>
          <button class="pms-btn is-ghost is-sm" type="button" onclick={() => fileInput?.click()}>
            <Icon name="file" size={14} />{ui.t('editor.totp_scan_image')}
          </button>
        {/if}
        <!-- 隐藏的原生文件选择：桌面与 Android 都走系统选择器，不需要额外插件权限。
             不设 capture，避免在手机上强制唤起相机（相册里的截图同样是我们需要的那条路）。
             放在分支之外：两个平台都要用，重复一份会让 `bind:this` 指向哪份变得不确定。 -->
        <input
          class="editor-scan-file"
          type="file"
          accept="image/*"
          bind:this={fileInput}
          onchange={(event) => void pickImage(event)}
        />
      </div>

      {#if scanOpen}
        <div class="editor-scan">
          {#if !scanCameraFailed}
            <video bind:this={scanVideo} class="editor-scan-preview" muted playsinline></video>
          {/if}
          <p class="pms-hint">{ui.t('editor.scan_hint')}</p>
          <div class="editor-scan-row">
            <button class="pms-btn is-ghost is-sm" type="button" onclick={closeScan}>
              {ui.t('common.cancel')}
            </button>
          </div>
        </div>
      {/if}

      <!-- 失败原因就地说明：拒绝路径（digits=8 / period=60 / HOTP）必须由界面直说，
           否则用户只会看到「扫了但没反应」，然后去怀疑摄像头。 -->
      {#if totpHint}
        <p class="pms-hint is-danger">{ui.t(totpHint.key, totpHint.params)}</p>
      {/if}
      {#if totpInvalid}
        <p class="pms-hint is-danger">{ui.t('err.totp_invalid')}</p>
      {/if}
    </div>

    <!-- D-89：备注编辑框。notes 状态与保存逻辑一直都在（导入的备注详情页可见），
         但表单从未有输入控件——数据「看得见改不了」。置于 TOTP 之后、自定义字段之前。 -->
    <div class="pms-field">
      <label class="pms-label" for="ed-notes">{ui.t('editor.field_notes')}</label>
      <textarea id="ed-notes" class="pms-input" rows="4" bind:value={notes}></textarea>
    </div>

    <div class="pms-field">
      <span class="pms-label">{ui.t('editor.custom_fields')}</span>
      <div class="pms-card editor-custom">
        {#each fields as field, index (index)}
          <div class="editor-custom-row">
            <input
              class="pms-input editor-key"
              placeholder={ui.t('editor.field_key')}
              aria-label={ui.t('editor.field_key')}
              bind:value={field.key}
            />
            <input
              class="pms-input editor-val"
              placeholder={ui.t('editor.field_value')}
              aria-label={ui.t('editor.field_value')}
              bind:value={field.value}
            />
            <button
              class="pms-icon-btn"
              type="button"
              aria-label={ui.t('common.remove')}
              onclick={() => removeField(index)}
            >
              <Icon name="close" size={14} />
            </button>
          </div>
        {/each}
        <button class="pms-btn is-ghost is-sm editor-add" type="button" onclick={addField}>
          <Icon name="plus" size={14} />{ui.t('editor.add_field')}
        </button>
      </div>
    </div>
  {:else}
    <div class="pms-field">
      <label class="pms-label" for="ed-body">{ui.t('note.body')}</label>
      <textarea
        id="ed-body"
        class="pms-input"
        rows="12"
        placeholder={ui.t('note.placeholder')}
        bind:value={body}
        aria-invalid={bodyTooLong}
      ></textarea>
      {#if bodyTooLong}
        <p class="pms-hint is-danger">{ui.t('note.too_long', { n: Math.floor(MAX_NOTE_BYTES / 1024) })}</p>
      {/if}
    </div>
  {/if}

  <div class="pms-field">
    <label class="pms-label" for="ed-folder">{ui.t('editor.folder')}</label>
    <select id="ed-folder" class="pms-input" bind:value={folderId}>
      <option value={null}>{ui.t('folder.ungrouped')}</option>
      {#each vault.folders as folder (folder.folder_id)}
        <option value={folder.folder_id}>{folder.name}</option>
      {/each}
    </select>
  </div>

  <label class="pms-check">
    <input type="checkbox" bind:checked={favorite} />
    <span>{ui.t('editor.favorite')}</span>
  </label>
  </div><!-- /.editor-scroll -->

  <div class="editor-actions">
    <button class="pms-btn is-ghost" type="button" onclick={oncancel}>{ui.t('editor.cancel')}</button>
    <button class="pms-btn is-primary" type="submit" disabled={!canSave}>{ui.t('editor.save')}</button>
  </div>
</form>

<!-- D-165：冻结快照的框选层。放在 form 之外：它是覆盖整个窗口的一层，
     夹在表单里会被 .editor-scroll 的滚动容器影响（fixed 不受，但语义上也该在外层）。 -->
{#if cropShot}
  <ScreenCrop shot={cropShot} hint={cropHint} onpick={(rect) => void onCropPick(rect)} oncancel={closeCrop} />
{/if}

<style>
  /* D-92：flex:1 桌面/移动统一生效——.pane 桌面端 display:contents，form.editor
     直接是 .body(flex row) 的 item，没有 flex:1 宽度就会按内容收缩成窄列。 */
  .editor {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  /* D-93：字段区独占滚动层；padding 归滚动层，操作条下方不再有空带。 */
  .editor-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 1.1rem;
  }
  .editor-custom {
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .editor-custom-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .editor-key {
    flex: 0 0 34%;
    min-height: 34px;
  }
  .editor-val {
    flex: 1;
    min-height: 34px;
  }
  .editor-add {
    align-self: flex-start;
  }
  /* D-162：扫码入口与预览面板。只做布局 —— 按钮/提示/圆角全部沿用既有类（§5.7）。 */
  .editor-scan-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.4rem;
  }
  .editor-scan {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-top: 0.5rem;
  }
  .editor-scan-preview {
    width: 100%;
    max-height: 220px;
    aspect-ratio: 4 / 3;
    object-fit: cover;
    background: #000;
    border-radius: var(--pms-r);
  }
  .editor-scan-file {
    display: none;
  }
  /* D-93：固定底栏（非 sticky）——flex 布局保证永远贴容器底，无缝隙、无内容透出。 */
  .editor-actions {
    flex-shrink: 0;
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding: 0.85rem 1.1rem;
    border-top: 1px solid var(--pms-border);
    background: var(--pms-bg);
  }
</style>
