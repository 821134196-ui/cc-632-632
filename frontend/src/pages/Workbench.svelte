<script>
  import { onMount } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';
  import { flash, user } from '../store.js';

  const dispatch = createEventDispatcher();
  let wb = null;
  let loading = true;

  async function load() {
    loading = true;
    try {
      wb = await api('/workbench');
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      loading = false;
    }
  }
  onMount(load);

  async function makeup(id) {
    if (!confirm('确认办理补赛检录？该学生状态将由缺席转为到场（补赛）。')) return;
    try {
      await api(`/checkins/${id}/makeup`, { method: 'POST', body: {} });
      flash('补赛检录已办理');
      await load();
    } catch (e) {
      flash(e.message, 'err');
    }
  }

  function openRound(roundId) {
    sessionStorage.setItem('open_round', String(roundId));
    dispatch('navigate', 'round');
    window.dispatchEvent(new CustomEvent('open-round', { detail: roundId }));
  }

  $: canCheckin = $user && ['checker', 'admin'].includes($user.role);
  $: n = wb
    ? wb.absent.length +
      wb.pending_review.length +
      wb.pending_corrections.length +
      wb.late.filter(x => !['confirmed', 'resolved'].includes(x.review_status)).length
    : 0;
</script>

{#if loading}
  <p class="muted">加载中…</p>
{:else}
  <div class="flex-between">
    <h1>工作台 {#if n > 0}<span class="badge danger">{n} 项待处理</span>{/if}</h1>
    <button class="sm" on:click={load}>刷新</button>
  </div>
  <p class="muted small mt8">
    本页汇总现场最需要处理的事项：缺席检录、补赛、电子计时与手记不一致待复核、待审核更正、漏记补录与尚未确认的成绩。
    <b>未确认的数据不会进入公示屏。</b>
  </p>

  <div class="grid cols-2 mt16">
    <div class="card">
      <h2>🚫 缺席检录 <span class="count {wb.absent.length ? '' : 'zero'}">{wb.absent.length}</span></h2>
      {#if wb.absent.length === 0}
        <p class="muted small">暂无缺席</p>
      {:else}
        <table>
          <thead><tr><th>号码</th><th>姓名</th><th>项目/轮次</th><th></th></tr></thead>
          <tbody>
            {#each wb.absent as a}
              <tr>
                <td>{a.bib}</td>
                <td>{a.student_name}<div class="muted small">{a.grade}{a.class}</div></td>
                <td>{a.event_name} {a.grp} · {a.round_label}{#if a.note}<div class="muted small">{a.note}</div>{/if}</td>
                <td class="row">
                  <button class="sm ok" disabled={!canCheckin} on:click={() => makeup(a.checkin_id)}>办理补赛</button>
                  <button class="sm" on:click={() => openRound(a.round_id)}>前往</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <div class="card">
      <h2>⏱ 电子/手记不一致 · 待总裁判复核 <span class="count {wb.pending_review.length ? '' : 'zero'}">{wb.pending_review.length}</span></h2>
      {#if wb.pending_review.length === 0}
        <p class="muted small">暂无争议成绩</p>
      {:else}
        <table>
          <thead><tr><th>号码</th><th>姓名</th><th class="num">电子</th><th class="num">手记</th><th></th></tr></thead>
          <tbody>
            {#each wb.pending_review as p}
              <tr>
                <td>{p.bib}</td>
                <td>{p.student_name}<div class="muted small">{p.event_name} · {p.round_label}</div></td>
                <td class="num">{p.electronic?.toFixed(2)}</td>
                <td class="num">{p.manual?.toFixed(2)}</td>
                <td><button class="sm danger" on:click={() => openRound(p.round_id)}>去复核</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="muted small mt8">系统不会自动选用任一读数，须由总裁判人工认定。</p>
      {/if}
    </div>

    <div class="card">
      <h2>📝 待审核的成绩更正 <span class="count {wb.pending_corrections.length ? '' : 'zero'}">{wb.pending_corrections.length}</span></h2>
      {#if wb.pending_corrections.length === 0}
        <p class="muted small">暂无待审核更正</p>
      {:else}
        <table>
          <thead><tr><th>学生</th><th class="num">原成绩</th><th class="num">改为</th><th>原因 / 发起人</th></tr></thead>
          <tbody>
            {#each wb.pending_corrections as c}
              <tr>
                <td>{c.student_name}<div class="muted small">{c.event_name} · {c.round_label}</div></td>
                <td class="num strike">{c.old_value.toFixed(2)}</td>
                <td class="num">{c.new_value.toFixed(2)}</td>
                <td>{c.reason}<div class="muted small">发起人：{c.requester_name}</div></td>
              </tr>
            {/each}
          </tbody>
        </table>
        <a class="small" href="#/corrections">前往成绩更正页审核 →</a>
      {/if}
    </div>

    <div class="card">
      <h2>🔁 补赛名单 <span class="count {wb.makeup.length ? '' : 'zero'}">{wb.makeup.length}</span></h2>
      {#if wb.makeup.length === 0}
        <p class="muted small">暂无补赛</p>
      {:else}
        <table>
          <thead><tr><th>号码</th><th>姓名</th><th>成绩状态</th><th></th></tr></thead>
          <tbody>
            {#each wb.makeup as m}
              <tr>
                <td>{m.bib}</td>
                <td>{m.student_name}<div class="muted small">{m.event_name} · {m.round_label}</div></td>
                <td>
                  {#if !m.review_status}
                    <span class="badge gray">待录成绩</span>
                  {:else if ['confirmed', 'resolved'].includes(m.review_status)}
                    <span class="badge ok">已确认 {m.final_value?.toFixed(2)}</span>
                  {:else}
                    <span class="badge warn">未确认（{m.review_status}）</span>
                  {/if}
                </td>
                <td><button class="sm" on:click={() => openRound(m.round_id)}>前往</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <div class="card">
      <h2>⏳ 漏记补录 <span class="count {wb.late.length ? '' : 'zero'}">{wb.late.length}</span></h2>
      {#if wb.late.length === 0}
        <p class="muted small">暂无漏记补录</p>
      {:else}
        <table>
          <thead><tr><th>号码</th><th>姓名</th><th>状态</th><th></th></tr></thead>
          <tbody>
            {#each wb.late as l}
              <tr>
                <td>{l.bib}</td>
                <td>{l.student_name}<div class="muted small">{l.event_name} · {l.round_label}{l.note ? ` · ${l.note}` : ''}</div></td>
                <td>
                  {#if ['confirmed','resolved'].includes(l.review_status)}
                    <span class="badge ok">已确认 {l.final_value?.toFixed(2)}</span>
                  {:else}
                    <span class="badge warn">未确认（{l.review_status}）</span>
                  {/if}
                </td>
                <td><button class="sm" on:click={() => openRound(l.round_id)}>前往</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <div class="card">
      <h2>❓ 未确认成绩 <span class="count {wb.unconfirmed.length ? '' : 'zero'}">{wb.unconfirmed.length}</span></h2>
      {#if wb.unconfirmed.length === 0}
        <p class="muted small">所有成绩均已确认</p>
      {:else}
        <table>
          <thead><tr><th>号码</th><th>姓名</th><th>状态</th><th></th></tr></thead>
          <tbody>
            {#each wb.unconfirmed as u}
              <tr>
                <td>{u.bib}</td>
                <td>{u.student_name}<div class="muted small">{u.event_name} · {u.round_label}{u.late ? ' · 漏记' : ''}</div></td>
                <td><span class="badge {u.review_status === 'pending' ? 'danger' : 'warn'}">{u.review_status}</span></td>
                <td><button class="sm" on:click={() => openRound(u.round_id)}>前往</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </div>
{/if}
