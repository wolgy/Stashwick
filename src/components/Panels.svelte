<script lang="ts">
  import { api, type StashEntry, type Worktree } from '../lib/api'
  import { app, repo, op, confirm, fmtDate, saveConfig, applyTheme, toast, showMenu, repoName, openWorktree, openInEditor, short } from '../lib/store.svelte'
  import { stashChanges, newWorktree, removeWorktree, lockWorktree, moveWorktree, pruneWorktrees } from '../lib/actions'

  // ───── worktrees ─────
  let worktrees = $state<Worktree[]>([])
  let wtError = $state('')
  $effect(() => {
    if (app.worktreesOpen && repo()) {
      app.logToken
      api.worktreeList(repo()!)
        .then((w) => ((worktrees = w), (wtError = '')))
        .catch((e) => ((worktrees = []), (wtError = String(e))))
    }
  })
  function openWt(w: Worktree) {
    app.worktreesOpen = false
    openWorktree(w.path)
  }
  function wtMenu(e: MouseEvent, w: Worktree) {
    showMenu(e, [
      { label: 'Open in Stashwick', disabled: w.current || w.bare || w.prunable !== null, action: () => openWt(w) },
      { label: 'Open in Editor', disabled: w.bare || w.prunable !== null, action: () => openInEditor([w.path]) },
      { label: 'Copy Path', action: () => navigator.clipboard.writeText(w.path) },
      '-',
      { label: w.locked !== null ? 'Unlock' : 'Lock…', disabled: w.main, action: () => lockWorktree(w) },
      { label: 'Move…', disabled: w.main || w.locked !== null, action: () => moveWorktree(w) },
      '-',
      { label: 'Remove…', danger: true, disabled: w.main, action: () => removeWorktree(w) },
    ])
  }
  const stale = $derived(worktrees.some((w) => w.prunable !== null))

  // ───── stashes ─────
  let stashes = $state<StashEntry[]>([])
  $effect(() => {
    if (app.stashOpen && repo()) {
      app.logToken
      api.stashList(repo()!).then((s) => (stashes = s)).catch(() => (stashes = []))
    }
  })
  async function stashAct(action: 'pop' | 'apply' | 'drop', s: StashEntry) {
    if (action === 'drop' && !(await confirm('Drop Stash', `Delete ${s.ref} "${s.message}"? This cannot be undone.`, 'Drop', true))) return
    await op(`Stash ${action} ${s.ref}`, (r) => api.stashAction(r, action, s.ref))
  }

  // ───── settings ─────
  let gitVersion = $state('')
  $effect(() => {
    if (app.settingsOpen) api.gitVersion().then((v) => (gitVersion = v)).catch((e) => (gitVersion = String(e)))
  })
  async function saveSettings() {
    await api.setGitPath(app.config.settings.gitPath)
    applyTheme()
    saveConfig()
    gitVersion = await api.gitVersion().catch((e) => String(e))
    toast('success', 'Settings saved')
  }
</script>

{#if app.stashOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="back" onclick={(e) => e.target === e.currentTarget && (app.stashOpen = false)}>
    <div class="panel">
      <div class="head row">
        <b class="grow">Stashes</b>
        <button onclick={stashChanges}>Stash Changes…</button>
        <button class="flat" onclick={() => (app.stashOpen = false)}>✕</button>
      </div>
      <div class="body">
        {#each stashes as s (s.ref)}
          <div class="srow row">
            <span class="mono dim">{s.ref}</span>
            <span class="grow ellipsis" title={s.message}>{s.message}</span>
            <span class="dim small">{fmtDate(s.time)}</span>
            <button onclick={() => stashAct('pop', s)} title="Apply and remove">Pop</button>
            <button onclick={() => stashAct('apply', s)} title="Apply, keep the stash">Apply</button>
            <button class="flat" onclick={() => stashAct('drop', s)} title="Delete">🗑</button>
          </div>
        {:else}
          <div class="empty">No stashes</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

{#if app.worktreesOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="back" onclick={(e) => e.target === e.currentTarget && (app.worktreesOpen = false)}>
    <div class="panel wide">
      <div class="head row">
        <b class="grow">Worktrees</b>
        {#if stale}<button onclick={pruneWorktrees} title="Forget worktrees whose folder was deleted">Prune Stale</button>{/if}
        <button onclick={() => newWorktree()}>New Worktree…</button>
        <button class="flat" onclick={() => (app.worktreesOpen = false)}>✕</button>
      </div>
      <div class="body">
        {#if wtError}<div class="empty err">{wtError}</div>{/if}
        {#each worktrees as w (w.path)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="srow row wrow" class:cur={w.current} oncontextmenu={(e) => wtMenu(e, w)} ondblclick={() => !w.current && w.prunable === null && !w.bare && openWt(w)}>
            <div class="grow wmain">
              <div class="row">
                <b class="ellipsis">{repoName(w.path)}</b>
                {#if w.main}<span class="tag">main</span>{/if}
                {#if w.current}<span class="tag on">current</span>{/if}
                {#if w.locked !== null}<span class="tag warn" title={w.locked || 'locked'}>🔒 locked</span>{/if}
                {#if w.prunable !== null}<span class="tag bad" title={w.prunable}>missing</span>{/if}
                <span class="mono branch ellipsis">
                  {#if w.bare}(bare){:else if w.branch}⎇ {w.branch}{:else}detached {w.head ? short(w.head) : ''}{/if}
                </span>
              </div>
              <div class="dim small mono ellipsis selectable" title={w.path}>{w.path}</div>
            </div>
            {#if !w.current && !w.bare && w.prunable === null}<button onclick={() => openWt(w)} title="Switch Stashwick to this worktree">Open</button>{/if}
            {#if !w.bare && w.prunable === null}<button class="flat" onclick={() => openInEditor([w.path])} title="Open in editor">✎</button>{/if}
            <button class="flat" onclick={(e) => wtMenu(e, w)} title="More">⋯</button>
          </div>
        {:else}
          {#if !wtError}<div class="empty">Loading…</div>{/if}
        {/each}
        <div class="dim small hintline">A branch can be checked out in only one worktree at a time. Double-click to open, right-click for more.</div>
      </div>
    </div>
  </div>
{/if}

{#if app.settingsOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="back" onclick={(e) => e.target === e.currentTarget && (app.settingsOpen = false)}>
    <div class="panel narrow">
      <div class="head row">
        <b class="grow">Settings</b>
        <button class="flat" onclick={() => (app.settingsOpen = false)}>✕</button>
      </div>
      <div class="body form">
        <label>
          <span>Git executable <span class="dim">(empty = “git” from PATH)</span></span>
          <input type="text" bind:value={app.config.settings.gitPath} placeholder="git" spellcheck="false" />
          <span class="dim small">{gitVersion}</span>
        </label>
        <label>
          <span>Editor command <span class="dim">(file paths are appended)</span></span>
          <input type="text" bind:value={app.config.settings.editor} placeholder="zed" spellcheck="false" />
          <span class="dim small">e.g. <code>zed</code>, <code>code</code>, <code>subl</code>, <code>idea</code></span>
        </label>
        <label>
          <span>Pull strategy</span>
          <select bind:value={app.config.settings.pullRebase}>
            <option value={false}>Merge</option>
            <option value={true}>Rebase</option>
          </select>
        </label>
        <label>
          <span>Log page size</span>
          <input type="text" bind:value={app.config.settings.logPageSize} />
        </label>
        <label>
          <span>Theme</span>
          <select bind:value={app.config.settings.theme} onchange={applyTheme}>
            <option value="auto">Follow system</option>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </label>
        <div class="row" style="justify-content:flex-end">
          <button class="primary" onclick={() => { app.config.settings.logPageSize = Math.max(100, Number(app.config.settings.logPageSize) || 1000); saveSettings(); app.settingsOpen = false }}>Save</button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if app.consoleOpen}
  <div class="console">
    <div class="head row">
      <b class="grow">Console</b>
      <button class="flat" onclick={() => (app.console = [])}>Clear</button>
      <button class="flat" onclick={() => (app.consoleOpen = false)}>✕</button>
    </div>
    <div class="cbody mono selectable">
      {#each app.console as c}
        <div class="centry" class:fail={!c.ok}>
          <div><span class="dim">{new Date(c.time).toLocaleTimeString()}</span> <b>{c.title}</b> <span class="dim">{c.repo}</span></div>
          {#if c.output}<pre>{c.output}</pre>{/if}
        </div>
      {:else}
        <div class="dim">No operations yet.</div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .back {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 60;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
  }
  .panel {
    width: 680px;
    max-width: 94vw;
    max-height: 72vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border2);
    border-radius: 8px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
  }
  .panel.wide {
    width: 760px;
  }
  .wrow {
    gap: 6px;
    padding: 6px 12px;
  }
  .wrow.cur {
    background: var(--sel);
  }
  .wmain {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .wmain .row {
    gap: 6px;
  }
  .branch {
    font-size: 12px;
    color: var(--fg2);
  }
  .tag {
    font-size: 10.5px;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border2);
    color: var(--fg2);
  }
  .tag.on {
    color: var(--accent);
    border-color: var(--accent);
  }
  .tag.warn {
    color: var(--warn);
    border-color: var(--warn);
  }
  .tag.bad,
  .err {
    color: var(--danger);
    border-color: var(--danger);
  }
  .hintline {
    padding: 8px 12px 4px;
  }
  .panel.narrow {
    width: 480px;
  }
  .head {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .body {
    overflow-y: auto;
    padding: 6px 0;
    min-height: 80px;
  }
  .srow {
    padding: 4px 12px;
  }
  .srow:hover {
    background: var(--hover);
  }
  .srow button {
    padding: 1px 8px;
    font-size: 12px;
  }
  .small {
    font-size: 11.5px;
  }
  .form {
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .form label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .console {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 24px;
    height: 34vh;
    z-index: 50;
    background: var(--panel);
    border-top: 1px solid var(--border2);
    display: flex;
    flex-direction: column;
    box-shadow: 0 -6px 20px rgba(0, 0, 0, 0.25);
  }
  .cbody {
    flex: 1;
    overflow-y: auto;
    padding: 6px 12px;
  }
  .centry {
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .centry.fail b {
    color: var(--danger);
  }
  .centry pre {
    margin: 2px 0 0;
    white-space: pre-wrap;
    color: var(--fg2);
  }
</style>
