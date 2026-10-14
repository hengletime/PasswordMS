<script lang="ts">
  // 密码生成器弹窗（客户："可以自定义生成位数、包含哪些字符" —— 原实现只挂在密码框右端一个 refresh 图标上、
  // 行内展开且无提示，客户完全没发现；现改为显式弹窗）。
  //
  // Modal 的真实 API（见 common/Modal.svelte）：{ open: boolean; titleKey: string; onclose?; children?; footer? }。
  // 注意：Modal 内部用 `{#if open}` 包裹整体 —— open=false 时本弹窗内的 PasswordGenerator 会**卸载**，
  // 其 $effect 随之销毁，因此关闭后不会再有未完成的生成请求，也不会残留错误 toast。
  import Modal from './common/Modal.svelte';
  import PasswordGenerator from './PasswordGenerator.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
    onuse?: (password: string) => void;
  }
  const { open, onclose, onuse }: Props = $props();
</script>

<Modal {open} titleKey="gen.title" {onclose}>
  <!-- onuse 缺省时（undefined）PasswordGenerator 只显示"复制"，不显示"使用此密码"，
       供"只想生成一个密码复制走"的场景使用。 -->
  <PasswordGenerator {onuse} />
</Modal>
