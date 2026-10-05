import { writable } from 'svelte/store';
import { api, setToken } from './api.js';

export const user = writable(null);
export const notice = writable(null);

export async function restoreSession() {
  try {
    const { user: u } = await api('/auth/me');
    user.set(u);
  } catch {
    setToken(null);
  }
}

export async function login(username, password) {
  const data = await api('/auth/login', {
    method: 'POST',
    body: { username, password }
  });
  setToken(data.token);
  user.set(data.user);
}

export async function logout() {
  try {
    await api('/auth/logout', { method: 'POST' });
  } finally {
    setToken(null);
    user.set(null);
  }
}

let timer;
export function flash(message, kind = 'ok') {
  notice.set({ message, kind });
  clearTimeout(timer);
  timer = setTimeout(() => notice.set(null), 3200);
}

export const ROLE_LABEL = {
  teacher: '老师',
  checker: '检录员',
  referee: '裁判',
  chief: '总裁判',
  admin: '管理员'
};

export const STATUS_LABEL = {
  raw: '待录入',
  agree: '双表一致·待确认',
  pending: '⚠ 计时不一致·待总裁判复核',
  resolved: '总裁判已复核',
  confirmed: '已确认'
};
