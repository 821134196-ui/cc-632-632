<script>
  import { login, flash } from '../store.js';

  let username = 'checker';
  let password = '123456';
  let busy = false;

  const quick = [
    ['teacher', '王老师', '报名'],
    ['checker', '李检录', '检录 / 补赛'],
    ['referee', '张裁判', '录成绩 / 发起更正'],
    ['referee2', '刘裁判', '审核更正（另一人）'],
    ['chief', '总裁判', '争议复核 / 发布榜单']
  ];

  async function submit() {
    busy = true;
    try {
      await login(username.trim(), password);
    } catch (e) {
      flash(e.message, 'err');
    } finally {
      busy = false;
    }
  }

  function pick(u) {
    username = u;
    password = '123456';
  }
</script>

<div class="login-wrap">
  <div class="card login-card">
    <h1>🏃 运动会检录与成绩系统</h1>
    <div class="sub">小学田径运动会 · 现场检录与成绩管理</div>
    <form on:submit|preventDefault={submit}>
      <label class="field">用户名
        <input bind:value={username} placeholder="用户名" />
      </label>
      <label class="field">密码
        <input type="password" bind:value={password} placeholder="密码" />
      </label>
      <button class="primary" disabled={busy}>{busy ? '登录中…' : '登 录'}</button>
    </form>
    <div class="quick-users">
      演示账号（密码均为 <code>123456</code>）：
      {#each quick as [u, name, duty]}
        <br /><button on:click={() => pick(u)}>{name}</button> · {duty}（<code>{u}</code>）
      {/each}
    </div>
  </div>
</div>
