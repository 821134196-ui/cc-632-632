<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { flash, user } from '../store.js';

  let students = [];
  let events = [];
  let rounds = [];
  let roundId = 1;
  let roster = [];

  let form = { student_id: '', bib: '' };
  let busy = false;

  async function loadBase() {
    const [s, e] = await Promise.all([api('/students'), api('/events')]);
    students = s;
    events = e.events;
    rounds = e.rounds;
  }
  async function loadRoster() {
    const r = await api(`/rounds/${roundId}/roster`);
    roster = r.entries;
  }

  onMount(async () => {
    await loadBase();
    await loadRoster();
  });

  $: eventOf = id => events.find(e => e.id === rounds.find(r => r.id === id)?.event_id);

  async function submit() {
    if (!form.student_id || !form.bib.trim()) {
      flash('请选择学生并填写号码布', 'err');
      return;
    }
    busy = true;
    try {
      await api('/registrations', {
        method: 'POST',
        body: { round_id: Number(roundId), student_id: Number(form.student_id), bib: form.bib.trim() }
      });
      flash('报名成功');
      form = { student_id: '', bib: '' };
      await loadRoster();
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      busy = false;
    }
  }

  async function changeRound() {
    await loadRoster();
  }

  $: canRegister = $user && ['teacher', 'admin'].includes($user.role);
</script>

<h1>学生报名</h1>

<div class="card">
  <div class="row">
    <label class="field">项目 / 组别 / 轮次
      <select bind:value={roundId} on:change={changeRound}>
        {#each rounds as r}
          {@const ev = events.find(e => e.id === r.event_id)}
          <option value={r.id}>{ev?.name} · {ev?.grp} · {r.label}</option>
        {/each}
      </select>
    </label>
  </div>
</div>

{#if canRegister}
  <div class="card">
    <h2>新增报名</h2>
    <form class="row" on:submit|preventDefault={submit}>
      <label class="field">学生
        <select bind:value={form.student_id}>
          <option value="">请选择…</option>
          {#each students as st}
            <option value={st.id}>{st.name}（{st.grade}{st.class}）</option>
          {/each}
        </select>
      </label>
      <label class="field">号码布
        <input bind:value={form.bib} placeholder="如 0310" maxlength="8" />
      </label>
      <button class="primary" disabled={busy}>报名</button>
    </form>
    <p class="muted small mt8">同一学生在同一轮次只能报名一次，重复提交将被系统拒绝。</p>
  </div>
{:else}
  <div class="card"><p class="muted">当前角色无报名权限，报名名单仅供查看。</p></div>
{/if}

<div class="card">
  <h2>报名名单（{roster.length} 人）</h2>
  <table>
    <thead>
      <tr><th>号码</th><th>姓名</th><th>年级班级</th><th>检录</th><th>成绩状态</th></tr>
    </thead>
    <tbody>
      {#each roster as e}
        <tr>
          <td>{e.bib}</td>
          <td>{e.student_name}</td>
          <td>{e.grade}{e.class}</td>
          <td>
            {#if !e.checkin_status}<span class="badge gray">未检录</span>
            {:else if e.checkin_status === 'present'}
              <span class="badge ok">到场{#if e.checkin_makeup}（补赛）{/if}</span>
            {:else}<span class="badge danger">缺席</span>{/if}
          </td>
          <td>{#if e.review_status}<span class="badge info">{e.review_status}</span>{:else}<span class="muted small">—</span>{/if}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
