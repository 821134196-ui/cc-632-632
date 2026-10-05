<script>
  import { onMount, onDestroy } from 'svelte';
  import { api } from '../api.js';
  import { flash, user, STATUS_LABEL } from '../store.js';

  let events = [];
  let rounds = [];
  let roundId = 1;
  let roster = [];
  let ranking = [];
  let loading = false;

  // 弹窗
  let dialog = null; // {type: 'manual'|'electronic'|'resolve'|'correct', entry}
  let form = { value: '', note: '', late: false, source: 'manual' };
  let busy = false;

  async function loadBase() {
    const e = await api('/events');
    events = e.events;
    rounds = e.rounds;
  }
  async function loadRoster() {
    loading = true;
    try {
      const r = await api(`/rounds/${roundId}/roster`);
      roster = r.entries;
      const rank = await api(`/rounds/${roundId}/ranking`);
      ranking = rank;
    } finally {
      loading = false;
    }
  }
  onMount(async () => {
    await loadBase();
    const stored = Number(sessionStorage.getItem('open_round'));
    if (stored && rounds.some(r => r.id === stored)) roundId = stored;
    sessionStorage.removeItem('open_round');
    await loadRoster();
    window.addEventListener('open-round', onOpenRound);
  });
  onDestroy(() => window.removeEventListener('open-round', onOpenRound));

  function onOpenRound(ev) {
    roundId = ev.detail;
    loadRoster();
  }

  $: evOf = id => events.find(e => e.id === rounds.find(r => r.id === id)?.event_id);
  $: unit = evOf(roundId)?.unit || '';

  $: isChecker = $user?.role === 'checker' || $user?.role === 'admin';
  $: isReferee = ['referee', 'chief', 'admin'].includes($user?.role);
  $: isChief = ['chief', 'admin'].includes($user?.role);

  async function checkin(entry, status) {
    const note = status === 'absent' ? prompt('缺席备注（可留空）', '') ?? '' : '';
    try {
      await api('/checkins', {
        method: 'POST',
        body: { registration_id: entry.registration_id, status, note }
      });
      flash(status === 'present' ? '检录到场成功' : '已登记缺席');
      await loadRoster();
    } catch (e) {
      flash(e.message, 'err');
    }
  }

  async function makeup(entry) {
    if (!confirm('确认办理补赛检录？该学生将由缺席转为到场（补赛）。')) return;
    try {
      await api(`/checkins/${entry.checkin_id}/makeup`, { method: 'POST', body: {} });
      flash('补赛检录已办理');
      await loadRoster();
    } catch (e) {
      flash(e.message, 'err');
    }
  }

  function openDialog(type, entry) {
    dialog = { type, entry };
    form = { value: '', note: '', late: false, source: 'manual' };
  }
  function close() { dialog = null; }

  async function submitDialog() {
    busy = true;
    const regId = dialog.entry.registration_id;
    try {
      if (dialog.type === 'manual') {
        if (!form.value || Number(form.value) <= 0) throw new Error('请输入有效成绩');
        await api('/results/manual', {
          method: 'POST',
          body: { registration_id: regId, value: Number(form.value), late: form.late, note: form.note }
        });
        flash('手记成绩已录入');
      } else if (dialog.type === 'electronic') {
        const v = await api('/results/electronic', {
          method: 'POST',
          body: { registration_id: regId, late: form.late, note: form.note }
        });
        flash(`电子计时读数：${v.electronic.toFixed(2)} ${unit}`);
      } else if (dialog.type === 'resolve') {
        if (!form.value || Number(form.value) <= 0) throw new Error('请输入认定成绩');
        if (!form.note.trim()) throw new Error('复核必须填写认定依据');
        await api(`/results/${dialog.entry.result_id}/resolve`, {
          method: 'POST',
          body: { final_value: Number(form.value), note: form.note }
        });
        flash('总裁判复核完成，成绩已确认');
      } else if (dialog.type === 'correct') {
        if (!form.value || Number(form.value) <= 0) throw new Error('请输入更正后的成绩');
        if (!form.note.trim()) throw new Error('更正必须填写原因');
        await api(`/results/${dialog.entry.result_id}/corrections`, {
          method: 'POST',
          body: { new_value: Number(form.value), reason: form.note.trim() }
        });
        flash('更正申请已提交，等待另一人审核');
      }
      close();
      await loadRoster();
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      busy = false;
    }
  }

  async function confirmResult(entry, source) {
    try {
      await api(`/results/${entry.result_id}/confirm`, {
        method: 'POST',
        body: source ? { use_single_source: source } : {}
      });
      flash('成绩已确认');
      await loadRoster();
    } catch (e) {
      flash(e.message, 'err');
    }
  }

  function statusBadge(s) {
    return {
      raw: ['gray', STATUS_LABEL.raw],
      agree: ['info', STATUS_LABEL.agree],
      pending: ['danger', STATUS_LABEL.pending],
      resolved: ['ok', STATUS_LABEL.resolved],
      confirmed: ['ok', STATUS_LABEL.confirmed]
    }[s] || ['gray', s];
  }
</script>

<h1>检录 / 成绩录入</h1>

<div class="card">
  <div class="row">
    <label class="field">项目 / 组别 / 轮次
      <select bind:value={roundId} on:change={loadRoster}>
        {#each rounds as r}
          {@const ev = events.find(e => e.id === r.event_id)}
          <option value={r.id}>{ev?.name} · {ev?.grp} · {r.label}</option>
        {/each}
      </select>
    </label>
    {#if evOf(roundId)}
      <span class="muted small">计量单位：{unit}；{evOf(roundId).higher_better ? '数值越大名次越前（田赛）' : '数值越小名次越前（径赛）'}</span>
    {/if}
    <button class="sm" style="margin-left:auto" on:click={loadRoster}>刷新</button>
  </div>
</div>

<div class="card">
  <h2>报名名单（{roster.length} 人）</h2>
  <p class="muted small">流程：检录到场 → 电子计时 / 手记 → 双表一致由裁判确认；不一致转总裁判复核。缺席者不参与排名，可办理补赛后再录成绩。</p>
  <div style="overflow-x:auto">
  <table>
    <thead>
      <tr>
        <th>号码</th><th>姓名</th><th>检录</th>
        <th class="num">电子</th><th class="num">手记</th>
        <th>状态</th><th class="num">最终成绩</th><th>操作</th>
      </tr>
    </thead>
    <tbody>
      {#each roster as e}
        <tr>
          <td>{e.bib}{#if e.late}<span class="badge warn" style="margin-left:4px">漏记</span>{/if}</td>
          <td>{e.student_name}<div class="muted small">{e.grade}{e.class}</div></td>
          <td>
            {#if !e.checkin_status}
              <span class="badge gray">未检录</span>
              {#if isChecker}
                <div class="row mt8">
                  <button class="sm ok" on:click={() => checkin(e, 'present')}>到场</button>
                  <button class="sm danger" on:click={() => checkin(e, 'absent')}>缺席</button>
                </div>
              {/if}
            {:else if e.checkin_status === 'present'}
              <span class="badge ok">到场{#if e.checkin_makeup}（补赛）{/if}</span>
            {:else}
              <span class="badge danger">缺席</span>
              {#if isChecker}
                <div class="mt8"><button class="sm ok" on:click={() => makeup(e)}>办理补赛</button></div>
              {/if}
            {/if}
          </td>
          <td class="num">{e.electronic != null ? e.electronic.toFixed(2) : '—'}</td>
          <td class="num">{e.manual != null ? e.manual.toFixed(2) : '—'}</td>
          <td>
            {#if e.review_status}
              <span class="badge {statusBadge(e.review_status)[0]}">{statusBadge(e.review_status)[1]}</span>
            {:else}
              <span class="muted small">—</span>
            {/if}
          </td>
          <td class="num">{#if e.final_value != null}<b>{e.final_value.toFixed(2)}</b>{:else}—{/if}</td>
          <td>
            {#if e.checkin_status === 'present' && isReferee}
              <div class="row">
                <button class="sm" on:click={() => openDialog('electronic', e)}>电子读数</button>
                <button class="sm" on:click={() => openDialog('manual', e)}>录手记</button>
                {#if e.review_status === 'agree'}
                  <button class="sm ok" on:click={() => confirmResult(e, null)}>确认</button>
                {:else if e.review_status === 'raw' && (e.electronic != null || e.manual != null)}
                  {#if e.electronic != null}<button class="sm ok" on:click={() => confirmResult(e, 'electronic')}>确认(电子)</button>{/if}
                  {#if e.manual != null}<button class="sm ok" on:click={() => confirmResult(e, 'manual')}>确认(手记)</button>{/if}
                {:else if e.review_status === 'pending' && isChief}
                  <button class="sm danger" on:click={() => openDialog('resolve', e)}>总裁判复核</button>
                {/if}
                {#if ['confirmed', 'resolved'].includes(e.review_status)}
                  <button class="sm" on:click={() => openDialog('correct', e)}>申请更正</button>
                {/if}
              </div>
            {:else if e.checkin_status !== 'present' && isChecker}
              <span class="muted small">先完成检录</span>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  </div>
</div>

<div class="card">
  <h2>当前排名预览（仅已确认成绩 · 未发布）</h2>
  {#if ranking.length === 0}
    <p class="muted small">尚无已确认成绩。该预览不会出现在公示屏。</p>
  {:else}
    <table>
      <thead><tr><th>名次</th><th>号码</th><th>姓名</th><th>年级班级</th><th class="num">成绩（{unit}）</th><th>备注</th></tr></thead>
      <tbody>
        {#each ranking as r}
          <tr>
            <td><b>{r.rank}</b>{#if r.tied}<span class="badge warn" style="margin-left:6px">并列</span>{/if}</td>
            <td>{r.bib}</td>
            <td>{r.student_name}</td>
            <td>{r.grade}{r.class}</td>
            <td class="num">{r.score.toFixed(2)}</td>
            <td>{#if r.makeup}<span class="badge info">补赛</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <p class="muted small mt8">确认无误后请到「榜单发布」页发布新榜单；公示屏只显示已发布的最新版本。</p>
</div>

{#if dialog}
  <div class="dialog-backdrop" on:click|self={close}>
    <div class="card dialog">
      <h2>
        {#if dialog.type === 'manual'}录入手记成绩
        {:else if dialog.type === 'electronic'}读取电子计时
        {:else if dialog.type === 'resolve'}总裁判复核（电子 {dialog.entry.electronic?.toFixed(2)} / 手记 {dialog.entry.manual?.toFixed(2)}）
        {:else}申请成绩更正（原成绩 {dialog.entry.final_value?.toFixed(2)} {unit}）{/if}
      </h2>
      <p class="muted small">{dialog.entry.bib} · {dialog.entry.student_name}</p>
      <form on:submit|preventDefault={submitDialog}>
        {#if dialog.type !== 'electronic'}
          <label class="field">
            {dialog.type === 'correct' ? '更正后成绩（' : '认定成绩（'}{unit}）
            <input type="number" step="0.01" min="0" bind:value={form.value} placeholder="0.00" />
          </label>
        {/if}
        {#if dialog.type !== 'resolve' && dialog.type !== 'correct'}
          <label class="field" style="flex-direction:row;align-items:center;gap:8px">
            <input type="checkbox" bind:checked={form.late} /> 漏记补录（赛后补登记，结果仍须确认）
          </label>
        {/if}
        <label class="field">{dialog.type === 'resolve' ? '认定依据（必填，记录原始争议情况）' : dialog.type === 'correct' ? '更正原因（必填）' : '备注'}
          <textarea rows="3" bind:value={form.note} placeholder={dialog.type === 'resolve' ? '例如：回看录像，手记起表偏晚，人工认定…' : dialog.type === 'correct' ? '例如：核对原始手记卡片，发现抄录错误…' : ''}></textarea>
        </label>
        {#if dialog.type === 'resolve'}
          <p class="small" style="color:var(--warn)">注意：系统不会自动选用电子或手记中的任何一个，最终成绩由总裁判人工认定；两份原始读数永久保留。</p>
        {:else if dialog.type === 'correct'}
          <p class="small muted">更正提交后须由另一名工作人员审核通过才生效；电子/手记原始记录不受影响。</p>
        {/if}
        <div class="row end">
          <button type="button" on:click={close}>取消</button>
          <button class="primary" disabled={busy}>
            {#if dialog.type === 'electronic'}读取电子计时{:else if dialog.type === 'correct'}提交更正申请{:else}提交{/if}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
