<script>
  import { onMount } from 'svelte';
  import { currentUser, toast } from './store.js';
  import { api } from './api.js';
  import { ROLE_LABEL } from './helpers.js';
  import Workbench from './pages/Workbench.svelte';
  import Teacher from './pages/Teacher.svelte';
  import Clerk from './pages/Clerk.svelte';
  import Referee from './pages/Referee.svelte';
  import HeadReferee from './pages/HeadReferee.svelte';
  import PublicBoard from './pages/PublicBoard.svelte';

  let users = [];
  let tab = 'workbench';

  const TABS = [
    { id: 'workbench', name: '工作台' },
    { id: 'teacher', name: '老师报名' },
    { id: 'clerk', name: '检录' },
    { id: 'referee', name: '裁判成绩' },
    { id: 'head', name: '总裁判复核' },
    { id: 'board', name: '📺 公示屏' },
  ];

  onMount(async () => {
    users = await api.get('/api/users');
    currentUser.set(users.find((u) => u.role === 'clerk') || users[0]);
  });

  function switchUser(id) {
    currentUser.set(users.find((u) => u.id === Number(id)));
  }
</script>

<header class="topbar">
  <h1><span class="logo">🏅</span>小学运动会检录与成绩系统</h1>
  <nav class="tabs">
    {#each TABS as t}
      <button class={tab === t.id ? 'active' : ''} on:click={() => (tab = t.id)}>{t.name}</button>
    {/each}
  </nav>
  <div class="userbox">
    {#if $currentUser}
      <span class="tiny">当前身份</span>
      <select value={$currentUser.id} on:change={(e) => switchUser(e.target.value)}>
        {#each users as u}
          <option value={u.id}>{u.display_name}（{ROLE_LABEL[u.role]}）</option>
        {/each}
      </select>
    {/if}
  </div>
</header>

<main>
  {#if tab === 'workbench'}
    <Workbench on:goto={(e) => (tab = e.detail)} />
  {:else if tab === 'teacher'}
    <Teacher />
  {:else if tab === 'clerk'}
    <Clerk />
  {:else if tab === 'referee'}
    <Referee />
  {:else if tab === 'head'}
    <HeadReferee />
  {:else if tab === 'board'}
    <PublicBoard />
  {/if}
</main>

{#if $toast}
  <div class="toast {$toast.type}">{$toast.msg}</div>
{/if}
