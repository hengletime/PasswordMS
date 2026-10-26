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
    // 2026-10-26：默认 forks 池的 worker loader（nodejs-worker-loader 的 ?vitest= 时间戳
    // 模块重写）在本机出现「runner 未注册 → reading 'config'」的 worker 状态异常，
    // 且与项目代码无关（零依赖最小测试同样复现；无配置时又正常）。vmForks 池走
    // VM 模块加载路径完全绕开该钩子，全量套件实测通过（42 文件 335 例），故固化。
    pool: 'vmForks',
  },
});
