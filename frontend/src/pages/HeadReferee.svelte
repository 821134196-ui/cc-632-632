<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { showToast } from '../store.js';
  import { fmtValue, SRC_LABEL } from '../helpers.js';

  let tab = 'queue';
  let queue = null;
  let rounds = [];
  let versions = [];
  let audit = [];
  let trace = null;
  let chosen = {}; // dispute_id -> chosen value
  let basis = {};
  let rejectReason = {};

  async function loadQueue() {
    queue = await api.get('/api/review/queue');
  }
  async function loadRounds() {
    rounds = await api.get('/api/rounds');
  }
  onMount(() => { loadQueue(); loadRounds(); });

  async function decide(d) {
    const v = parseFloat(chosen[d.dispute_id]);
    const b = (basis[d.dispute_id] || '').trim();
    if (!v) return showToast('请选择裁决采用值', 'error');
    if (!b) return showToast('必须填写裁决依据', 'error');
    try {
      await api.post(`/api/review/disputes/${d.dispute_id}`, { chosen_value: v, basis: b });
      showToast('分歧已裁决，成绩确认', 'success');
      chosen[d.dispute_id] = ''; basis[d.dispute_id] = '';
      await loadQueue();
    } catch (e) { showToast(e.message, 'error'); }
  }
  async function confirmResult(rid, kind) {
    try {
      await api.post(`/api/review/results/${rid}/confirm`, { note: kind === 'reshoot' ? '补赛确认' : '漏记补录确认' });
      showToast(kind === 'reshoot' ? '补赛成绩已确认' : '漏记补录成绩已确认', 'success');
      await loadQueue();
    } catch (e) { showToast(e.message, 'error'); }
  }
  async function rejectResult(rid) {
    const reason = (rejectReason[rid] || '').trim();
    if (!reason) return showToast('驳回必须填写原因', 'error');
    try {
      await api.post(`/api/review/results/${rid}/reject`, { reason });
      showToast('已驳回，该成绩不参与排名', 'success');
      rejectReason[rid] = '';
      await loadQueue();
    } catch (e) { showToast(e.message, 'error'); }
  }
  async function reviewCorrection(c, decision) {
    try {
      await api.post(`/api/corrections/${c.id}/review`, { decision });
      showToast(decision === 'approved' ? '更正已通过：请重新发布榜单，公示屏才会更新' : '更正已驳回', decision === 'approved' ? 'success' : 'info');
      await loadQueue();
    } catch (e) { showToast(e.message, 'error'); }
  }
  async function publish(roundId, note) {
    try {
      const r = await api.post(`/api/rounds/${roundId}/leaderboard/publish`, { note });
      showToast(`榜单 v${r.version} 已发布；旧版本继续保留`, 'success');
      await loadRounds();
      await loadVersions(roundId);
    } catch (e) { showToast(e.message, 'error'); }
  }
  let pubNotes = {};
  async function loadVersions(roundId) {
    versions = await api.get(`/api/rounds/${roundId}/leaderboard/versions`);
  }
  let versionRoundId = null;
  async function pickRound(id) {
    versionRoundId = id;
    await loadVersions(id);
  }
  async function openTrace(rid) {
    trace = await api.get(`/api/results/${rid}/trace`);
  }
  async function loadAudit() {
    audit = await api.get('/api/audit');
  }
  function switchTab(t) {
    tab = t;
    if (t === 'audit' && !audit.length) loadAudit();
    if (t === 'publish') loadRounds();
  }
</script>

<div class="pill-tabs">
  <button class={tab === 'queue' ? 'active' : ''} on:click={() => switchTab('queue')}>
    复核队列（{(queue ? queue.disputes.length + queue.pending_results.length + queue.corrections.length : 0)}）
  </button>
  <button class={tab === 'publish' ? 'active' : ''} on:click={() => switchTab('publish')}>榜单发布与历史</button>
  <button class={tab === 'audit' ? 'active' : ''} on:click={() => switchTab('audit')}>操作审计</button>
</div>

{#if queue && tab === 'queue'}
  <div class="card">
    <h2>⚖️ 电子计时 / 手记分歧（{queue.disputes.length}）</h2>
    <p class="subtitle">系统不会自动选择任何一个值；必须由总裁判裁决并填写依据。</p>
    {#if queue.disputes.length === 0}<p class="muted">无待裁决分歧。</p>{/if}
    {#each queue.disputes as d}
      <div style="border:1px solid var(--line);border-radius:10px;padding:12px 14px;margin-bottom:10px">
        <div class="row" style="justify-content:space-between">
          <div><b>{d.student_name}</b>（{d.bib_number}） · {d.event_name} · {d.group_name} · {d.round_name}
            {#if d.is_reshoot}<span class="badge orange">补赛</span>{/if}
          </div>
        </div>
        <div class="row" style="margin:8px 0">
          <label class="field"><input type="radio" name="d{d.dispute_id}" bind:group={chosen[d.dispute_id]} value={String(d.electronic_val)} />
            电子计时 <b class="mono">{fmtValue(d.electronic_val, d.unit)}</b></label>
          <label class="field"><input type="radio" name="d{d.dispute_id}" bind:group={chosen[d.dispute_id]} value={String(d.manual_val)} />
            手记 <b class="mono">{fmtValue(d.manual_val, d.unit)}</b></label>
        </div>
        <div class="row">
          <input style="flex:1" placeholder="裁决依据（必填，如：核对终点影像，电子计时有效）" bind:value={basis[d.dispute_id]} />
          <button class="btn primary" on:click={() => decide(d)}>裁决确认</button>
        </div>
      </div>
    {/each}
  </div>

  <div class="card">
    <h2>📝 漏记补录 / 补赛成绩确认（{queue.pending_results.length}）</h2>
    {#if queue.pending_results.length === 0}<p class="muted">无待确认成绩。</p>{/if}
    <table>
      <tr><th>类型</th><th>项目/轮次</th><th>学生</th><th>电子</th><th>手记</th><th class="right">操作</th></tr>
      {#each queue.pending_results as p}
        <tr>
          <td>{#if p.kind === 'reshoot'}<span class="badge orange">补赛</span>{:else}<span class="badge blue">漏记补录</span>{/if}</td>
          <td>{p.event_name} · {p.group_name}<div class="tiny muted">{p.round_name}</div></td>
          <td>{p.student_name} <span class="tiny muted">{p.bib_number}</span></td>
          <td class="mono">{fmtValue(p.electronic_val, p.unit)}</td>
          <td class="mono">{fmtValue(p.manual_val, p.unit)}</td>
          <td class="right nowrap">
            <button class="btn sm ghost" on:click={() => openTrace(p.result_id)}>溯源</button>
            <button class="btn sm primary" on:click={() => confirmResult(p.result_id, p.kind)}>确认采用</button>
            <input style="width:110px" placeholder="驳回原因" bind:value={rejectReason[p.result_id]} />
            <button class="btn sm red" on:click={() => rejectResult(p.result_id)}>驳回</button>
          </td>
        </tr>
      {/each}
    </table>
  </div>

  <div class="card">
    <h2>✍️ 成绩更正审核（{queue.corrections.length}）</h2>
    <p class="subtitle">更正必须由申请人之外的另一人审核；本人审核会被拒绝。</p>
    {#if queue.corrections.length === 0}<p class="muted">无待审核更正。</p>{/if}
    {#each queue.corrections as c}
      <div style="border:1px solid var(--line);border-radius:10px;padding:12px 14px;margin-bottom:10px">
        <div><b>{c.student_name}</b> · {c.event_name} · {c.group_name}</div>
        <div style="margin:6px 0">
          <span class="mono">{Number(c.old_value).toFixed(2)}</span>
          <span class="muted"> → </span>
          <b class="mono">{Number(c.new_value).toFixed(2)}</b>
        </div>
        <div class="tiny muted">申请人：{c.requester_name}（{c.requested_at}）</div>
        <div style="margin:6px 0">原因：{c.reason}</div>
        <div class="row" style="justify-content:flex-end">
          <button class="btn red sm" on:click={() => reviewCorrection(c, 'rejected')}>驳回</button>
          <button class="btn primary sm" on:click={() => reviewCorrection(c, 'approved')}>我是另一人，审核通过</button>
        </div>
      </div>
    {/each}
  </div>
{/if}

{#if tab === 'publish'}
  <div class="card">
    <h2>📢 发布榜单</h2>
    <p class="subtitle">仅包含<b>已确认</b>且到场的成绩；缺席、待复核、被驳回的成绩一律不进入榜单。每次发布生成新版本。</p>
    <table>
      <tr><th>轮次</th><th>项目/组别</th><th>已发布版本</th><th class="right">发布新版本</th></tr>
      {#each rounds as r}
        <tr>
          <td>{r.name}{r.is_reshoot ? ' <span class="badge orange">补赛</span>' : ''}</td>
          <td>{r.event_name} · {r.group_name}</td>
          <td>{#if r.published_version}v{r.published_version}{:else}<span class="muted">未发布</span>{/if}</td>
          <td class="right">
            <div class="row" style="justify-content:flex-end">
              <input style="width:170px" placeholder="版本说明（可选）" bind:value={pubNotes[r.id]} />
              <button class="btn primary sm" on:click={() => publish(r.id, pubNotes[r.id])}>发布新榜单</button>
              <button class="btn ghost sm" on:click={() => pickRound(r.id)}>历史版本</button>
            </div>
          </td>
        </tr>
      {/each}
    </table>
  </div>

  {#if versionRoundId}
    <div class="card">
      <h2>🗂️ 历史榜单版本</h2>
      <p class="subtitle">新版本发布后旧榜单永久保留快照；点击查看可核对成绩与名次。</p>
      {#if versions.length === 0}<p class="muted">该轮次尚未发布过榜单。</p>{/if}
      {#each versions as v}
        <div class="row" style="justify-content:space-between;border-bottom:1px solid var(--line);padding:8px 0">
          <div>
            <b>v{v.version}</b> <span class="tiny muted">{v.published_at} · 发布人 {v.publisher_name || '—'} · {v.row_count} 人</span>
            <div class="tiny muted">{v.note || '（无说明）'}</div>
          </div>
          <a class="btn ghost sm" href={`/api/leaderboard/${v.version_id}`} target="_blank">查看快照 JSON</a>
        </div>
      {/each}
    </div>
  {/if}
{/if}

{#if tab === 'audit'}
  <div class="card">
    <h2>📜 操作审计（最近 200 条）</h2>
    <p class="subtitle">检录、计时、裁决、更正、发布等关键动作均留痕，可与原始计时记录、历史榜单相互核对。</p>
    <table>
      <tr><th>时间</th><th>操作人</th><th>动作</th><th>详情</th></tr>
      {#each audit as a}
        <tr>
          <td class="tiny nowrap">{a.created_at}</td>
          <td>{a.actor || '—'}</td>
          <td><code class="ev tiny">{a.action}</code></td>
          <td class="tiny">{a.detail}</td>
        </tr>
      {/each}
    </table>
  </div>
{/if}

{#if trace}
  <div class="modal-mask" on:click={(e) => e.target.classList.contains('modal-mask') && (trace = null)}>
    <div class="modal">
      <h3>成绩溯源 · {trace.result.student_name}</h3>
      <p class="subtitle">{trace.result.event_name} · {trace.result.group_name} · {trace.result.round_name}</p>
      <h3>原始计时记录（不可变）</h3>
      <table>
        <tr><th>来源</th><th>数值</th><th>记录人</th><th>时间</th><th>备注</th></tr>
        {#each trace.raw_results as r}
          <tr><td>{SRC_LABEL[r.source] || r.source}</td><td class="mono">{fmtValue(r.value, trace.result.unit)}</td>
              <td>{r.recorder || '—'}</td><td class="tiny">{r.recorded_at}</td><td class="tiny">{r.note}</td></tr>
        {/each}
      </table>
      <h3>更正历史</h3>
      {#if trace.corrections.length === 0}<p class="muted">无更正。</p>{/if}
      {#each trace.corrections as c}
        <div style="margin-bottom:8px">
          <span class="badge {c.status === 'approved' ? 'green' : c.status === 'rejected' ? 'red' : 'orange'}">
            {c.status === 'approved' ? '已通过' : c.status === 'rejected' ? '已驳回' : '待审核'}
          </span>
          <span class="mono">{c.old_value} → {c.new_value}</span>
          <div class="tiny">申请人 {c.requester_name}{c.reviewer_name ? ` / 审核人 ${c.reviewer_name}` : ''}</div>
          <div class="tiny muted">原因：{c.reason}</div>
        </div>
      {/each}
      <div class="row" style="justify-content:flex-end"><button class="btn ghost" on:click={() => (trace = null)}>关闭</button></div>
    </div>
  </div>
{/if}
