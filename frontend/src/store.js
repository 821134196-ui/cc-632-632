import { writable } from 'svelte/store';

// 当前登录工作人员（由顶部切换，对应后端 X-User-Id）
export const currentUser = writable(null);

// 全局轻提示
export const toast = writable(null);
let toastTimer;
export function showToast(msg, type = 'info') {
  toast.set({ msg, type });
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.set(null), 3500);
}
