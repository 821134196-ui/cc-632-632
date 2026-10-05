<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { showToast } from '../store.js';

  let students = [], events = [], entries = [];
  let kw = '';
  let newSt = { name: '', grade: 1, class: '1班', bib_number: '' };
  let newEv = { name: '', group_name: '', unit: 'seconds', sort_rank: 1 };
  let selectedEvent = null;
  let pickedStudent = '';

  async function load() {
    [students, events, entries] = await Promise.all([
      api.get('/api/students?q=' + encodeURIComponent(kw)),
      api.get('/api/events'),
      api.get('/api/entries'),
    ]);
    if (!selectedEvent && events.length) selectedEvent = events[0].id;
  }
  onMount(load);

  async function addStudent() {
    if (!newSt.name || !newSt.bib_number) return showToast('姓名和号码布必填', 'error');
    try {
      await api.post('/api/students', newSt);
      showToast(`学生 ${newSt.name} 已建档`, 'success');
      newSt = { name: '', grade: 1, class: '1班', bib_number: '' };
      await load();
    } catch (e) { showToast(e.message, 'error'); }
  }

  async function addEvent() {
    if (!newEv.name || !newEv.group_name) return showToast('项目名称和组别必填', 'error');
    try {
      await api.post('/api/events', newEv);
      showToast('项目已创建（自动生成决赛轮次）', 'success');
      newEv = { name: '', group_name: '', unit: 'seconds', sort_rank: 1 };
      await load();
    } catch (e) { showToast(e.message, 'error'); }
  }

  async function enter() {
    if (!selectedEvent || !pickedStudent) return showToast('请选择学生', 'error');
    try {
      await api.post('/api/entries', { event_id: selectedEvent, student_id: Number(pickedStudent) });
      showToast('报名成功', 'success');
      pickedStudent = '';
      await load();
    } catch (e) { showToast(e.message, 'error'); }
  }

  $: eventEntries = selectedEvent
    ? entries.filter((e) => e.event_id === selectedEvent)
    : [];
  $: enteredIds = new Set(eventEntries.map((e) => e.student_id));
  $: availableStudents = students.filter((s) => !enteredIds.has(s.id));
  $: currentEvent = events.find((e) => e.id === selectedEvent);
</script>

<div class="grid2">
  <div class="card">
    <h2>👦 学生档案</h2>
    <p class="subtitle">老师维护参赛学生名册。</p>
    <div class="row" style="margin-bottom:10px">
      <input placeholder="搜索姓名 / 号码布" bind:value={kw} on:input={load} />
    </div>
    <div class="row" style="margin-bottom:12px;align-items:flex-end">
      <label class="field">姓名<input bind:value={newSt.name} placeholder="如 陈一" /></label>
      <label class="field">年级
        <select bind:value={newSt.grade}>
          {#each Array(6) as _, i}<option value={i + 1}>{i + 1}年级</option>{/each}
        </select>
      </label>
      <label class="field">班级<input bind:value={newSt.class} placeholder="1班" /></label>
      <label class="field">号码布<input bind:value={newSt.bib_number} placeholder="A1006" /></label>
      <button class="btn primary" on:click={addStudent}>建档</button>
    </div>
    <table>
      <tr><th>号码布</th><th>姓名</th><th>年级班级</th></tr>
      {#each students as s}
        <tr><td class="mono">{s.bib_number}</td><td>{s.name}</td><td>{s.grade}年级 {s.class}</td></tr>
      {/each}
    </table>
  </div>

  <div class="card">
    <h2>🏟️ 比赛项目</h2>
    <div class="row" style="margin-bottom:12px;align-items:flex-end">
      <label class="field">项目名称<input bind:value={newEv.name} placeholder="如 60米 / 跳远" /></label>
      <label class="field">组别<input bind:value={newEv.group_name} placeholder="一年级男子组" /></label>
      <label class="field">类型
        <select bind:value={newEv.unit} on:change={() => (newEv.sort_rank = newEv.unit === 'seconds' ? 1 : -1)}>
          <option value="seconds">径赛（时间越短越好）</option>
          <option value="meters">田赛（距离越大越好）</option>
        </select>
      </label>
      <button class="btn primary" on:click={addEvent}>新增项目</button>
    </div>
    <table>
      <tr><th>项目</th><th>组别</th><th>类型</th><th>报名数</th></tr>
      {#each events as e}
        <tr>
          <td>{e.name}</td><td>{e.group_name}</td>
          <td><span class="badge {e.unit === 'seconds' ? 'blue' : 'orange'}">{e.unit === 'seconds' ? '径赛' : '田赛'}</span></td>
          <td class="mono">{e.entry_count}</td>
        </tr>
      {/each}
    </table>
  </div>
</div>

<div class="card">
  <h2>📝 项目报名</h2>
  <p class="subtitle">老师录入学生报名项目；同一学生重复报同一项目会被拒绝。</p>
  <div class="row" style="margin-bottom:12px">
    <label class="field">选择项目
      <select bind:value={selectedEvent}>
        {#each events as e}
          <option value={e.id}>{e.name} · {e.group_name}（已报 {e.entry_count} 人）</option>
        {/each}
      </select>
    </label>
    <label class="field">添加学生
      <select bind:value={pickedStudent}>
        <option value="">请选择未报名学生…</option>
        {#each availableStudents as s}
          <option value={s.id}>{s.bib_number} · {s.name}（{s.grade}年级{s.class}）</option>
        {/each}
      </select>
    </label>
    <button class="btn primary" on:click={enter}>报名</button>
  </div>
  {#if currentEvent}
    <h3>{currentEvent.name} · {currentEvent.group_name} 报名名单（{eventEntries.length}）</h3>
    <table>
      <tr><th>号码布</th><th>姓名</th><th>年级班级</th></tr>
      {#each eventEntries as en}
        <tr><td class="mono">{en.bib_number}</td><td>{en.student_name}</td><td>{en.grade}年级 {en.class}</td></tr>
      {/each}
    </table>
  {/if}
</div>
