<script>
  import { onMount, onDestroy } from 'svelte';
  import { api } from '../api.js';

  let data = null;
  let timer;
  let now = new Date();
  let clockTimer;

  async function load() {
    data = await api.get('/api/public-board');
  }
  onMount(() => {
    load();
    timer = setInterval(load, 5000);
    clockTimer = setInterval(() => (now = new Date()), 1000);
    return () => { clearInterval(timer); clearInterval(clockTimer); };
  });
  onDestroy(() => {});

  const unitLabel = (u) => (u === 'seconds' ? '秒' : '米');
</script>

<div class="board-screen">
  <div class="row" style="justify-content:space-between;align-items:baseline;margin-bottom:16px">
    <h2 style="margin:0;font-size:26px">📺 成绩公示屏</h2>
    <div class="tiny" style="opacity:.75">
      仅显示<b>已发布</b>的确认成绩 · 每 5 秒刷新 · {now.toLocaleString('zh-CN')}
    </div>
  </div>

  {#if data}
    {@const published = data.rounds.filter((r) => r.published)}
    {@const waiting = data.rounds.filter((r) => !r.published)}
    <div class="board-grid">
      {#each published as r}
        <div class="board-card">
          <h3>{r.event_name} · {r.group_name}</h3>
          <div class="meta">
            {r.round_name}{#if r.is_reshoot}（补赛）{/if} · 第 {r.version} 版 · {r.published_at}
          </div>
          <table>
            <tr><th>名次</th><th>号码</th><th>姓名</th><th>班级</th><th class="right">成绩</th></tr>
            {#each r.rows as row}
              <tr>
                <td class="rank-{row.rank}">{row.rank === 1 ? '🥇' : row.rank === 2 ? '🥈' : row.rank === 3 ? '🥉' : ''} {row.rank}</td>
                <td class="mono">{row.bib_number}</td>
                <td>{row.student_name}</td>
                <td class="tiny">{row.grade}年级{row.class}</td>
                <td class="right mono">{Number(row.value).toFixed(2)} {unitLabel(r.unit)}</td>
              </tr>
            {/each}
          </table>
        </div>
      {/each}
    </div>

    {#if waiting.length}
      <div class="tiny" style="opacity:.6;margin-top:22px">
        以下项目尚无已发布榜单（检录/计时/复核进行中，未确认成绩不会公示）：
        {#each waiting as r, i}{i ? '、' : ''}{r.event_name}（{r.group_name}·{r.round_name}）{/each}
      </div>
    {/if}
  {/if}
</div>
