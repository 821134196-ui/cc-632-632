<script>
  import { onMount } from 'svelte';
  import { user, notice, restoreSession, logout, ROLE_LABEL } from './store.js';
  import Login from './pages/Login.svelte';
  import Workbench from './pages/Workbench.svelte';
  import Register from './pages/Register.svelte';
  import RoundPage from './pages/RoundPage.svelte';
  import Corrections from './pages/Corrections.svelte';
  import Publications from './pages/Publications.svelte';
  import Screen from './pages/Screen.svelte';

  let route = 'workbench';

  function syncHash() {
    const h = location.hash.replace(/^#\/?/, '');
    route = h || 'workbench';
  }

  onMount(async () => {
    syncHash();
    window.addEventListener('hashchange', syncHash);
    if (location.hash !== '#/screen') {
      await restoreSession();
    }
  });

  function go(r) {
    location.hash = `#/${r}`;
  }

  $: isScreen = route === 'screen';
</script>

{#if isScreen}
  <Screen />
{:else if !$user}
  <Login />
{:else}
  <div class="topbar">
    <div class="logo">🏃 校运动会检录系统</div>
    <nav>
      <a href="#/workbench" class:active={route === 'workbench'}>工作台</a>
      <a href="#/register" class:active={route === 'register'}>报名</a>
      <a href="#/round" class:active={route === 'round'}>检录 / 成绩</a>
      <a href="#/corrections" class:active={route === 'corrections'}>成绩更正</a>
      <a href="#/publications" class:active={route === 'publications'}>榜单发布</a>
      <a href="#/screen" target="_blank" rel="noreferrer">公示屏 ↗</a>
    </nav>
    <div class="userbox">
      <span>{$user.display_name}（{ROLE_LABEL[$user.role] || $user.role}）</span>
      <button class="sm" on:click={logout}>退出</button>
    </div>
  </div>

  <div class="container">
    {#if $notice}
      <div class="notice {$notice.kind === 'err' ? 'err' : 'ok'}">{$notice.message}</div>
    {/if}

    {#if route === 'workbench'}
      <Workbench on:navigate={e => go(e.detail)} />
    {:else if route === 'register'}
      <Register />
    {:else if route === 'round'}
      <RoundPage />
    {:else if route === 'corrections'}
      <Corrections />
    {:else if route === 'publications'}
      <Publications />
    {/if}
  </div>
{/if}
