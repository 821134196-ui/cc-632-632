<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { showToast } from '../store.js';
  import { confBadge, fmtValue, SRC_LABEL } from '../helpers.js';

  let rounds = [];
  let roundId = null;
  let round = null;
  let roster = [];
  let busyId = null;
  let manualVals = {};
  let elecVals = {};
  let missingVals = {};
  let correctTarget = null; // {result_id, name, value}
  let corr = { new_value: '', reason: '' };

  async function loadRounds() {
    rounds = await api.get('/api/rounds');
    if (!roundId && rounds.length) roundId = rounds[0].id;
    await loadRoster();
  }
  async function loadRoster() {
    const d = await api.get(`/api/rounds/${roundId}/roster`);
    round = d.round;
    roster = d.roster;
  }
  onMount(loadRounds);

  async function record(checkinId, source) {
    const val = parseFloat(source === 'electronic' ? elecVals[checkinId] : manualVals[checkinId]);
    if (!val || val <= 0) return showToast('请输入有效成绩', 'error');
    busyId = checkinId;
    try {
      const r = await api.post('/api/timing', { checkin_id: checkinId, source, value: val });
      if (r.state === 'auto_confirmed') showToast('电子/手记一致，成绩已自动确认', 'success');
      else if (r.state === 'dispute') showToast('电子计时与手记差值超容差，已提交总裁判复核（暂不发布）', 'error');
      else showToast('已记录，等待另一来源计时', 'success');
      await loadRoster();
    } catch (e) { showToast(e.message, 'error'); }
    finally { busyId = null; }
  }

  async function simulate(checkinId, force) {
    busyId = checkinId;
    try {
      const r = await api.post('/api/timer/simulate', { checkin_id: checkinId, force_dispute: force });
      if (r.reconcile.state === 'auto_confirmed') showToast(`本地计时器：电子 ${r.electronic} / 手记 ${r.manual}，一致已确认`, 'success');
      else showToast(`本地计时器：电子 ${r.electronic} / 手记 ${r.manual}，存在分歧 → 待总裁判复核`, 'error');
      await loadRoster();
    } catch (e) { showToast(e.message, 'error'); }
    finally { busyId = null; }
  }

  async function missing(checkinId) {
    const val = parseFloat(missingVals[checkinId]);
    if (!val || val <= 0) return showToast('请输入补录成绩', 'error');
    busyId = checkinId;
    try {
      await api.post('/api/results/missing', { checkin_id: checkinId, value: val, note: '现场漏记，赛后补录' });
      showToast('漏记补录已提交，等待总裁判确认', 'success');
      missingVals[checkinId] = '';
      await loadRoster();
    } catch (e) { showToast(e.message, 'error'); }
    finally { busyId = null; }
  }

  function openCorrect(row) {
    correctTarget = row;
    corr = { new_value: String(row.published_value ?? ''), reason: '' };
  }
  async function submitCorrection() {
    const v = parseFloat(corr.new_value);
    if (!v || v <= 0) return showToast('请输入有效新成绩', 'error');
    if (!corr.reason.trim()) return showToast('必须填写更正原因', 'error');
    try {
      await api.post('/api/corrections', { result_id: correctTarget.result_id, new_value: v, reason: corr.reason });
      showToast('更正申请已提交，等待另一人审核（审核通过并重新发布后才上公示屏）', 'success');
      correctTarget = null;
      await loadRoster();
    } catch (e) { showToast(e.message, 'error'); }
  }

  function hasSource(row, src) {
    return row.raw_results.some((r) => r.source === src);
  }
</script>

<div class="card">
  <h2>⏱️ 裁判成绩录入</h2>
  <p class="subtitle">
    电子计时与手记均为<b>不可变原始记录</b>；两者差值在容差
    （径赛 0.2 秒 / 田赛 0.02 米）以内自动确认，超出则交总裁判复核，<b>系统不自行选择发布</b>。
    可使用「本地模拟计时器」一键生成两条记录。
  </p>
  <label class="field" style="max-width:420px">轮次
    <select bind:value={roundId} on:change={loadRoster}>
      {#each rounds as r}
        <option value={r.id}>{r.event_name} · {r.group_name} — {r.name}{r.is_reshoot ? '（补赛）' : ''}</option>
      {/each}
    </select>
  </label>
</div>

{#if round}
  <div class="card">
    <h2>{round.event_name} · {round.group_name} — {round.name}</h2>
    <table>
      <tr>
        <th>号码布</th><th>姓名</th><th>电子计时</th><th>手记</th>
        <th>确认状态</th><th>采用成绩</th><th class="right">操作</th>
      </tr>
      {#each roster.filter((x) => x.checkin_status !== 'absent') as row}
        <tr>
          <td class="mono">{row.bib_number}</td>
          <td>{row.student_name}{#if !row.checkin_status}<div class="tiny muted">尚未检录</div>{/if}</td>
          <td>
            {#each row.raw_results.filter((r) => r.source === 'electronic') as r}
              <div class="mono">{fmtValue(r.value, round.unit)} <span class="tiny muted">{r.note || ''}</span></div>
            {/each}
            {#if row.checkin_status === 'present' && !hasSource(row, 'electronic')}
              <div class="row"><input style="width:88px" type="number" step="0.01" placeholder="电子值" bind:value={elecVals[row.checkin_id]} />
                <button class="btn sm ghost" disabled={busyId === row.checkin_id} on:click={() => record(row.checkin_id, 'electronic')}>录入</button></div>
            {/if}
            {#if !row.checkin_status}<span class="muted tiny">—</span>{/if}
          </td>
          <td>
            {#each row.raw_results.filter((r) => r.source === 'manual') as r}
              <div class="mono">{fmtValue(r.value, round.unit)} <span class="tiny muted">{r.note || ''}</span></div>
            {/each}
            {#if row.checkin_status === 'present' && !hasSource(row, 'manual')}
              <div class="row"><input style="width:88px" type="number" step="0.01" placeholder="手记值" bind:value={manualVals[row.checkin_id]} />
                <button class="btn sm ghost" disabled={busyId === row.checkin_id} on:click={() => record(row.checkin_id, 'manual')}>录入</button></div>
            {/if}
            {#if !row.checkin_status}<span class="muted tiny">—</span>{/if}
          </td>
          <td>
            {#if row.confirmation}<span class="badge {confBadge(row.confirmation)[0]}">{confBadge(row.confirmation)[1]}</span>{:else}—{/if}
          </td>
          <td class="mono">{fmtValue(row.published_value, round.unit)}</td>
          <td class="right nowrap">
            {#if row.checkin_status === 'present'}
              {#if row.raw_results.length === 0}
                <button class="btn sm primary" disabled={busyId === row.checkin_id} on:click={() => simulate(row.checkin_id, false)}>🌀 模拟计时</button>
                <button class="btn sm orange" disabled={busyId === row.checkin_id} on:click={() => simulate(row.checkin_id, true)}>模拟(制造分歧)</button>
                <div class="row" style="margin-top:4px;justify-content:flex-end">
                  <input style="width:78px" type="number" step="0.01" placeholder="漏记补录" bind:value={missingVals[row.checkin_id]} />
                  <button class="btn sm ghost" disabled={busyId === row.checkin_id} on:click={() => missing(row.checkin_id)}>漏记补录</button>
                </div>
              {:else}
                {#if row.confirmation === 'confirmed'}
                  <button class="btn sm ghost" on:click={() => openCorrect(row)}>申请更正</button>
                {:else if row.confirmation === 'pending'}
                  <span class="tiny muted">等待总裁判复核…</span>
                {/if}
              {/if}
            {/if}
          </td>
        </tr>
      {/each}
      {#each roster.filter((x) => x.checkin_status === 'absent') as row}
        <tr style="opacity:.55">
          <td class="mono">{row.bib_number}</td><td>{row.student_name}</td>
          <td colspan="4"><span class="badge red">缺席 · 不参与排名</span></td><td></td>
        </tr>
      {/each}
    </table>
  </div>
{/if}

{#if correctTarget}
  <div class="modal-mask" on:click={(e) => e.target.classList.contains('modal-mask') && (correctTarget = null)}>
    <div class="modal">
      <h3>成绩更正申请 · {correctTarget.student_name}</h3>
      <p class="subtitle">原始计时记录不可更改；更正必须说明原因，并由<b>另一人</b>审核后生效。</p>
      <div class="row" style="margin-bottom:10px">
        <label class="field">当前成绩
          <input value={fmtValue(correctTarget.published_value, round.unit)} disabled />
        </label>
        <label class="field">更正为（{round.unit === 'seconds' ? '秒' : '米'}）
          <input type="number" step="0.01" bind:value={corr.new_value} />
        </label>
      </div>
      <label class="field" style="margin-bottom:12px">更正原因（必填）
        <textarea rows="3" bind:value={corr.reason} placeholder="如：终点摄像复核发现电子计时板误差 / 手记转写错误…"></textarea>
      </label>
      <div class="row" style="justify-content:flex-end">
        <button class="btn ghost" on:click={() => (correctTarget = null)}>取消</button>
        <button class="btn primary" on:click={submitCorrection}>提交更正申请</button>
      </div>
    </div>
  </div>
{/if}
