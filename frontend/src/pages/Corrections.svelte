<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { flash, user } from '../store.js';

  let list = [];
  let loading = true;

  async function load() {
    loading = true;
    try {
      const r = await api('/corrections');
      list = r.corrections;
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      loading = false;
    }
  }
  onMount(load);

  async function review(id, decision) {
    const review_note = prompt(
      decision === 'approved'
        ? '审核意见（可留空）：通过后成绩将更新，原始计时记录仍保留，需要重新发布榜单'
        : '驳回原因（可留空）',
      ''
    );
    if (review_note === null) return;
    try {
      await api(`/corrections/${id}/review`, { method: 'POST', body: { decision, review_note } });
      flash(decision === 'approved' ? '更正已通过，请重新发布榜单' : '已驳回');
      await load();
    } catch (e) {
      flash(e.message, 'err');
    }
  }

  const STATUS = {
    pending: ['warn', '待另一人审核'],
    approved: ['ok', '已通过'],
    rejected: ['danger', '已驳回']
  };
  function statusOf(s) {
    return STATUS[s] || ['gray', s];
  }

  $: canReview = ['referee', 'chief', 'admin'].includes($user?.role);
</script>

<h1>成绩更正</h1>
<div class="card">
  <p class="muted small">
    更正规则：只有<b>已确认</b>的成绩才能发起更正，必须填写原因；更正须由<b>另一个人</b>审核，
    本人不能审核自己的申请。更正通过后只更新排名用的最终成绩，电子/手记原始记录永久保留；
    新榜单发布后，旧榜单仍然可以随时查阅。
  </p>
</div>

<div class="card">
  <div class="flex-between">
    <h2>更正申请记录（{list.length}）</h2>
    <button class="sm" on:click={load}>刷新</button>
  </div>
  {#if loading}
    <p class="muted">加载中…</p>
  {:else if list.length === 0}
    <p class="muted small">暂无更正记录。</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>学生</th><th>项目</th>
          <th class="num">原成绩</th><th class="num">更正为</th>
          <th>原因</th><th>发起人</th><th>状态 / 审核人</th><th></th>
        </tr>
      </thead>
      <tbody>
        {#each list as c}
          {@const st = statusOf(c.status)}
          <tr>
            <td>{c.student_name}</td>
            <td>{c.event_name} · {c.round_label}</td>
            <td class="num strike">{c.old_value.toFixed(2)}</td>
            <td class="num"><b>{c.new_value.toFixed(2)}</b></td>
            <td style="max-width:220px">{c.reason}{#if c.review_note}<div class="muted small">审核意见：{c.review_note}</div>{/if}</td>
            <td>{c.requester_name}</td>
            <td>
              <span class="badge {st[0]}">{st[1]}</span>
              {#if c.reviewer_name}<div class="muted small">审核：{c.reviewer_name}</div>{/if}
            </td>
            <td>
              {#if c.status === 'pending' && canReview}
                <div class="row">
                  <button class="sm ok" on:click={() => review(c.id, 'approved')}>通过</button>
                  <button class="sm danger" on:click={() => review(c.id, 'rejected')}>驳回</button>
                </div>
                <p class="muted small mt8">若你是发起人，系统将拒绝你的审核。</p>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
