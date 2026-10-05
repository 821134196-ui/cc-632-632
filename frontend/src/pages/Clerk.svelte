<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { showToast } from '../store.js';
  import { confBadge, fmtValue } from '../helpers.js';

  let rounds = [];
  let roundId = null;
  let roster = null;
  let round = null;
  let busy = false;
  let newRoundName = '';

  async function loadRounds() {
    rounds = await api.get('/api/rounds');
    if (!roundId && rounds.length) roundId = rounds[0].id;
    if (roundId) await loadRoster();
  }
  async function loadRoster() {
    const d = await api.get(`/api/rounds/${roundId}/roster`);
    round = d.round;
    roster = d.roster;
  }
  onMount(loadRounds);

  async function checkin(entryId, status) {
    const note = status === 'absent' ? prompt('缺席原因（可选）', '') : '';
    if (note === null) return;
    busy = true;
    try {
      await api.post('/api/checkins', { entry_id: entryId, round_id: roundId, status, note });
      showToast(status === 'present' ? '已检录到场' : '已登记缺席（不参与排名）', 'success');
      await loadRoster();
      await loadRounds();
    } catch (e) {
      showToast(e.message, 'error');
    } finally { busy = false; }
  }

  async function addReshoot() {
    const name = (newRoundName || '').trim() || '补赛';
    try {
      const r = await api.post('/api/rounds', { event_id: round.event_id, name, is_reshoot: true });
      showToast('补赛轮次已创建', 'success');
      newRoundName = '';
      await loadRounds();
      roundId = r.id;
      await loadRoster();
    } catch (e) { showToast(e.message, 'error'); }
  }
</script>

<div class="card">
  <h2>✅ 现场检录</h2>
  <p class="subtitle">
    按项目、组别、轮次登记到场/缺席。<b>同一学生在同一轮次只能检录一次</b>，重复提交会被系统拒绝；
    缺席者不参与排名。补赛请新建补赛轮次。
  </p>
  <div class="row">
    <label class="field">轮次
      <select bind:value={roundId} on:change={loadRoster}>
        {#each rounds as r}
          <option value={r.id}>
            {r.event_name} · {r.group_name} — {r.name}{r.is_reshoot ? '（补赛）' : ''}
          </option>
        {/each}
      </select>
    </label>
    {#if round}
      <label class="field">新建补赛轮次
        <div class="row">
          <input placeholder="轮次名（默认：补赛）" bind:value={newRoundName} />
          <button class="btn orange" on:click={addReshoot}>＋ 补赛</button>
        </div>
      </label>
    {/if}
  </div>
</div>

{#if round && roster}
  <div class="card">
    <h2>{round.event_name} · {round.group_name} — {round.name}{round.is_reshoot ? '（补赛轮次）' : ''}</h2>
    <p class="subtitle">
      共 {roster.length} 名报名学生；
      已到场 {roster.filter((x) => x.checkin_status === 'present').length}，
      缺席 {roster.filter((x) => x.checkin_status === 'absent').length}，
      未检录 {roster.filter((x) => !x.checkin_status).length}
      {#if round.is_reshoot}<span class="badge orange">补赛成绩需总裁判确认</span>{/if}
    </p>
    <table>
      <tr>
        <th>号码布</th><th>姓名</th><th>年级班级</th><th>检录状态</th>
        <th>成绩状态</th><th>成绩</th><th class="right">操作</th>
      </tr>
      {#each roster as row}
        <tr>
          <td class="mono">{row.bib_number}</td>
          <td>{row.student_name}</td>
          <td>{row.grade}年级{row.class}</td>
          <td>
            {#if row.checkin_status === 'present'}<span class="badge green">到场</span>
            {:else if row.checkin_status === 'absent'}<span class="badge red">缺席</span>
            {:else}<span class="badge gray">未检录</span>{/if}
          </td>
          <td>
            {#if row.checkin_status === 'absent'}<span class="muted tiny">不参与排名</span>
            {:else if row.confirmation}
              <span class="badge {confBadge(row.confirmation)[0]}">{confBadge(row.confirmation)[1]}</span>
            {:else}—{/if}
          </td>
          <td class="mono">{fmtValue(row.published_value, round.unit)}</td>
          <td class="right nowrap">
            {#if !row.checkin_status}
              <button class="btn sm primary" disabled={busy} on:click={() => checkin(row.entry_id, 'present')}>到场</button>
              <button class="btn sm red" disabled={busy} on:click={() => checkin(row.entry_id, 'absent')}>缺席</button>
            {:else}
              <span class="tiny muted">已检录，不可重复</span>
            {/if}
          </td>
        </tr>
      {/each}
    </table>
  </div>
{/if}
