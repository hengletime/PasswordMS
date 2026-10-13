import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// Svelte 5：启用 vitePreprocess 以支持 <script lang="ts"> 与内联样式。
export default {
  preprocess: vitePreprocess(),
};
