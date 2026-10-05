export const ROLE_LABEL = {
  teacher: '老师',
  clerk: '检录员',
  referee: '裁判',
  head_referee: '总裁判',
};

export function fmtValue(v, unit) {
  if (v === null || v === undefined) return '—';
  const n = Number(v);
  return unit === 'seconds' ? `${n.toFixed(2)} 秒` : `${n.toFixed(2)} 米`;
}

export function confBadge(c) {
  return {
    pending: ['orange', '待复核'],
    confirmed: ['green', '已确认'],
    rejected: ['red', '已驳回'],
  }[c] || ['gray', c || '未开始'];
}

export const SRC_LABEL = { electronic: '电子计时', manual: '手记' };
