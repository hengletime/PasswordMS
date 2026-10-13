import { mount } from 'svelte';
import App from './App.svelte';
import './styles/global.css';

// 应用引导入口：把根组件挂载到 #app。
// 说明：T01 阶段仅提供最小可构建骨架；完整路由与状态在 T09~T15 实现。
const target = document.getElementById('app');
if (!target) {
  // 挂载点缺失属于开发期装配错误，直接抛出让引导阶段尽早暴露问题。
  throw new Error('PasswordMS: mount target #app not found');
}

const app = mount(App, { target });

export default app;
