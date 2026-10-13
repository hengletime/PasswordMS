import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// PasswordMS 前端构建配置（两端共用）。
// 体积优先：单 chunk、关闭 sourcemap、esbuild 压缩。
export default defineConfig({
  plugins: [svelte()],
  // Tauri CLI 依赖固定端口，避免自动切换导致 devUrl 失配
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2022',
    minify: 'esbuild',
    sourcemap: false,
    cssCodeSplit: false,
    reportCompressedSize: true,
    chunkSizeWarningLimit: 600,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    globals: false,
  },
});
