<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { flash, user } from '../store.js';

  let events = [];
  let rounds = [];
  let roundId = 1;
  let ranking = [];
  let publications = [];
  let detail = null;
  let busy = false;

  async function loadBase() {
    const e = await api('/events');
    events = e.events;
    rounds = e.rounds;
  }
  async function loadAll() {
    const [r, p] = await Promise.all([
      api(`/rounds/${roundId}/ranking`),
      api(`/rounds/${roundId}/publications`)
    ]);
    ranking = r;
    publications = p.publications;
  }
  onMount(async () => {
    await loadBase();
    await loadAll();
  });

  $: ev = id => events.find(e => e.id === rounds.find(r => r.id === id)?.event_id);
  $: isChief = ['chief', 'admin'].includes($user?.role);

  async function publish() {
    if (ranking.length === 0) {
      flash('还没有已确认成绩，不能发布', 'err');
      return;
    }
    if (!confirm(`确认发布第 ${publications.length + 1} 版榜单？发布后公示屏将立即更新，旧版本继续保留。`)) return;
    busy = true;
    try {
      const v = await api(`/rounds/${roundId}/publish`, { method: 'POST', body: {} });
      flash(`第 ${v.version} 版榜单已发布`);
      await loadAll();
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      busy = false;
    }
  }

  async function showDetail(id) {
    detail = await api(`/publications/${id}`);
  }
</script>

<h1>榜单发布</h1>

<div class="card">
  <div class="row">
    <label class="field">项目 / 组别 / 轮次
      <select bind:value={roundId} on:change={loadAll}>
        {#each rounds as r}
          {@const e = events.find(x => x.id === r.event_id)}
          <option value={r.id}>{e?.name} · {e?.grp} · {r.label}</option>
        {/each}
      </select>
    </label>
    <button class="primary" disabled={busy || !isChief} on:click={publish} style="margin-left:auto">
      发布新榜单（当前第 {publications.length} 版）
    </button>
  </div>
  {#if !isChief}<p class="muted small mt8">仅总裁判可发布榜单，当前为只读查看。</p>{/if}
</div>

<div class="grid cols-2 mt16">
  <div class="card">
    <h2>待发布排名（已确认成绩）</h2>
    {#if ranking.length === 0}
      <p class="muted small">没有已确认成绩。未确认、漏记未核、补赛未确认的学生都不会进入榜单。</p>
    {:else}
      <table>
        <thead><tr><th>名次</th><th>号码</th><th>姓名</th><th class="num">成绩（{ev(roundId)?.unit}）</th><th>备注</th></tr></thead>
        <tbody>
          {#each ranking as r}
            <tr>
              <td><b>{r.rank}</b>{#if r.tied}<span class="badge warn" style="margin-left:6px">并列</span>{/if}</td>
              <td>{r.bib}</td><td>{r.student_name}</td>
              <td class="num">{r.score.toFixed(2)}</td>
              <td>{#if r.makeup}<span class="badge info">补赛</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <div class="card">
    <h2>历史版本（{publications.length}）</h2>
    {#if publications.length === 0}
      <p class="muted small">尚未发布过榜单。</p>
    {:else}
      <table>
        <thead><tr><th>版本</th><th>发布时间(UTC)</th><th>人数</th><th>发布人</th><th></th></tr></thead>
        <tbody>
          {#each publications as p}
            <tr>
              <td><b>第 {p.version} 版</b>{#if p.version === publications[0].version}<span class="badge ok" style="margin-left:6px">公示中</span>{/if}</td>
              <td class="small">{p.created_at}</td>
              <td>{p.entry_count}</td>
              <td class="small">{p.created_by_name || '—'}</td>
              <td><button class="sm" on:click={() => showDetail(p.id)}>查看</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>

{#if detail}
  <div class="dialog-backdrop" on:click|self={() => (detail = null)}>
    <div class="card dialog" style="width:620px">
      <div class="flex-between">
        <h2>{detail.publication.event_name} · {detail.publication.grp} · {detail.publication.round_label} — 第 {detail.publication.version} 版</h2>
        <button class="sm" on:click={() => (detail = null)}>关闭</button>
      </div>
      <p class="muted small">发布于 {detail.publication.created_at}（历史快照，不随后续更正变化）</p>
      <table>
        <thead><tr><th>名次</th><th>号码</th><th>姓名</th><th>年级班级</th><th class="num">成绩（{detail.publication.unit}）</th><th>备注</th></tr></thead>
        <tbody>
          {#each detail.entries as e}
            <tr>
              <td><b>{e.rank}</b>{#if e.tied}<span class="badge warn" style="margin-left:6px">并列</span>{/if}</td>
              <td>{e.bib}</td><td>{e.student_name}</td><td>{e.grade}{e.class}</td>
              <td class="num">{e.score.toFixed(2)}</td>
              <td>{#if e.makeup}<span class="badge info">补赛</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}
