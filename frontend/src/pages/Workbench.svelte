<script>
  import { onMount } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';
  import { fmtValue } from '../helpers.js';

  const dispatch = createEventDispatcher();
  let data = null;
  let timer;

  async function load() {
    try {
      data = await api.get('/api/workbench');
    } catch (e) { /* toast 已提示 */ }
  }
  onMount(() => {
    load();
    timer = setInterval(load, 8000);
    return () => clearInterval(timer);
  });
</script>

<div class="subtitle">工作台突出展示需要处理的缺席、待复核（分歧 / 漏记 / 补赛）与成绩更正，每 8 秒自动刷新。</div>

{#if data}
  <div class="row" style="margin-bottom:16px">
    <div class="stat warn"><div class="n">{data.counts.absentees}</div><div class="l">缺席记录</div></div>
    <div class="stat alert"><div class="n">{data.counts.open_disputes}</div><div class="l">电子/手记分歧待裁决</div></div>
    <div class="stat alert"><div class="n">{data.counts.pending_results}</div><div class="l">漏记/补赛等待确认</div></div>
    <div class="stat warn"><div class="n">{data.counts.pending_corrections}</div><div class="l">成绩更正待另一人审核</div></div>
    <div class="stat"><button class="btn ghost" on:click={load}>刷新</button></div>
  </div>

  <div class="grid2">
    <div class="card">
      <h2>🚩 待复核项目（{data.pending_results.length}）</h2>
      <p class="subtitle">未确认成绩绝不会进入公示屏；补赛与漏记需总裁判确认。</p>
      {#if data.pending_results.length === 0}
        <p class="muted">暂无待复核项目。</p>
      {:else}
        <table>
          <tr><th>项目 / 组别</th><th>学生</th><th>情况</th><th></th></tr>
          {#each data.pending_results as p}
            <tr>
              <td>{p.event_name} · {p.group_name}<div class="tiny muted">{p.round_name}{p.is_reshoot ? '（补赛）' : ''}</div></td>
              <td>{p.student_name}<div class="tiny muted">{p.bib_number}</div></td>
              <td>
                {#if p.has_dispute}<span class="badge red">电子/手记分歧</span>
                {:else if p.is_reshoot}<span class="badge orange">补赛待确认</span>
                {:else}<span class="badge orange">漏记待确认</span>{/if}
              </td>
              <td class="right"><button class="btn sm primary" on:click={() => dispatch('goto', 'head')}>去处理</button></td>
            </tr>
          {/each}
        </table>
      {/if}
    </div>

    <div class="card">
      <h2>🏃 缺席名单（{data.absentees.length}）</h2>
      <p class="subtitle">缺席者不参与排名，也不能录入成绩。</p>
      {#if data.absentees.length === 0}
        <p class="muted">暂无缺席记录。</p>
      {:else}
        <table>
          <tr><th>项目 / 轮次</th><th>学生</th><th>备注</th></tr>
          {#each data.absentees as a}
            <tr>
              <td>{a.event_name} · {a.group_name}<div class="tiny muted">{a.round_name}</div></td>
              <td>{a.student_name} <span class="tiny muted">{a.bib_number}</span></td>
              <td class="tiny muted">{a.note || '—'}</td>
            </tr>
          {/each}
        </table>
      {/if}
    </div>
  </div>

  <div class="card">
    <h2>🔁 补赛轮次（{data.reshoot_rounds.length}）</h2>
    {#if data.reshoot_rounds.length === 0}
      <p class="muted">暂无补赛轮次。检录员可在检录页为项目新建补赛轮次。</p>
    {:else}
      <table>
        <tr><th>项目</th><th>组别</th><th>轮次</th><th>已到场人数</th><th></th></tr>
        {#each data.reshoot_rounds as r}
          <tr>
            <td>{r.event_name}</td><td>{r.group_name}</td><td>{r.name}</td>
            <td class="mono">{r.present_n}</td>
            <td class="right">
              <button class="btn sm ghost" on:click={() => dispatch('goto', 'clerk')}>去检录</button>
              <button class="btn sm primary" on:click={() => dispatch('goto', 'head')}>去确认</button>
            </td>
          </tr>
        {/each}
      </table>
    {/if}
  </div>
{/if}
