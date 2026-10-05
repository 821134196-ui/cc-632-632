<script>
  import { onMount, onDestroy } from 'svelte';
  import { publicApi } from '../api.js';

  let boards = [];
  let timer;
  let updated = '';

  async function load() {
    const data = await publicApi('/public/screen');
    boards = data.boards || [];
    updated = new Date().toLocaleTimeString('zh-CN', { hour12: false });
  }
  onMount(() => {
    load();
    timer = setInterval(load, 5000);
  });
  onDestroy(() => clearInterval(timer));
</script>

<div class="screen">
  <h1>🏅 运动会成绩公示</h1>
  <div class="sub">仅公布已确认并发布的成绩 · 自动刷新 · 更新于 {updated}</div>

  {#if boards.length === 0}
    <div class="screen-empty">
      暂无已发布榜单<br />
      <span style="font-size:14px">成绩须经裁判确认 / 总裁判复核并发布后才会在此公布</span>
    </div>
  {:else}
    <div class="screen-grid">
      {#each boards as b}
        <div class="screen-board">
          <h2>
            {b.board.event_name} · {b.board.grp}
            <span class="ver">第 {b.board.version} 版 · {b.board.round_label}</span>
          </h2>
          <table>
            <thead>
              <tr><th>名次</th><th>号码</th><th>姓名</th><th>班级</th><th class="num">成绩（{b.board.unit}）</th></tr>
            </thead>
            <tbody>
              {#each b.entries as e}
                <tr>
                  <td class="rank{e.rank <= 3 ? e.rank : ''}">
                    {#if e.rank === 1}🥇{:else if e.rank === 2}🥈{:else if e.rank === 3}🥉{:else}{e.rank}{/if}
                    {#if e.tied}<span class="tie-flag">并列</span>{/if}
                  </td>
                  <td>{e.bib}</td>
                  <td>{e.student_name}{#if e.makeup}<span class="tie-flag">补</span>{/if}</td>
                  <td class="small">{e.grade}{e.class}</td>
                  <td class="num">{e.score.toFixed(2)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/each}
    </div>
  {/if}
</div>
