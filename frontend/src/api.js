import { get } from 'svelte/store';
import { currentUser, showToast } from './store.js';

const BASE = '';

async function request(method, path, body) {
  const u = get(currentUser);
  const headers = { 'Content-Type': 'application/json' };
  if (u) headers['X-User-Id'] = String(u.id);
  let res;
  try {
    res = await fetch(BASE + path, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch (e) {
    showToast('网络错误：无法连接服务器', 'error');
    throw e;
  }
  const text = await res.text();
  const data = text ? JSON.parse(text) : {};
  if (!res.ok) {
    const msg = data.error || `请求失败 HTTP ${res.status}`;
    const err = new Error(msg);
    err.status = res.status;
    throw err;
  }
  return data;
}

export const api = {
  get: (p) => request('GET', p),
  post: (p, b) => request('POST', p, b ?? {}),
};
