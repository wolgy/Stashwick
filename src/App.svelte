<script lang="ts">
  import { onMount } from 'svelte'
  import {
    app, initApp, refreshAll, refreshStatus, refreshSummaries, repoName, workspace, selectRepo, mod, modLabel, short,
  } from './lib/store.svelte'
  import { fetchAll, pull, pullDialog, pushDialog, sequencer, stashChanges } from './lib/actions'
  import { splitter, sizeStore } from './lib/ui'
  import Sidebar from './components/Sidebar.svelte'
  import CommitView from './components/CommitView.svelte'
  import LogView from './components/LogView.svelte'
  import BranchesPopup from './components/BranchesPopup.svelte'
  import MergeTool from './components/MergeTool.svelte'
  import Panels from './components/Panels.svelte'
  import Overlays from './components/Overlays.svelte'

  const sideW = sizeStore('sidebar', 240)
  let side = $state(sideW.get())
  let s0 = 0

  const st = $derived(app.status)
  const conflictCount = $derived(st?.files.filter((f) => f.conflict).length ?? 0)
  const changeCount = $derived(st?.files.length ?? 0)
  const current = $derived(app.branches.find((b) => b.kind === 'local' && b.current))

  onMount(() => {
    initApp()
    const onFocus = () => refreshAll()
    window.addEventListener('focus', onFocus)
    // light polling while focused, so edits made in your editor show up quickly
    const t1 = setInterval(() => document.hasFocus() && !app.busy && refreshStatus(), 4000)
    const t2 = setInterval(() => !app.busy && refreshSummaries(), 30000)
    return () => {
      window.removeEventListener('focus', onFocus)
      clearInterval(t1)
      clearInterval(t2)
    }
  })

  function onKey(e: KeyboardEvent) {
    if (app.dialog || app.mergeFile) return
    const k = e.key.toLowerCase()
    if (e.key === 'F5' || (mod(e) && k === 'r')) {
      e.preventDefault()
      refreshAll()
    } else if (mod(e) && e.shiftKey && k === 'k') {
      e.preventDefault()
      pushDialog()
    } else if (mod(e) && k === 'k') {
      e.preventDefault()
      app.view = 'commit'
      setTimeout(() => document.querySelector<HTMLTextAreaElement>('.commitbox textarea')?.focus())
    } else if (mod(e) && k === 't') {
      e.preventDefault()
      pull()
    } else if (mod(e) && k === 'b') {
      e.preventDefault()
      app.branchesOpen = !app.branchesOpen
    } else if (mod(e) && k === 'l') {
      e.preventDefault()
      app.view = 'log'
    } else if (mod(e) && /^[1-9]$/.test(e.key)) {
      const r = workspace().repos[Number(e.key) - 1]
      if (r) {
        e.preventDefault()
        selectRepo(r)
      }
    }
  }

  const stateLabel: Record<string, string> = {
    merging: 'Merge in progress',
    rebasing: 'Rebase in progress',
    'cherry-picking': 'Cherry-pick in progress',
    reverting: 'Revert in progress',
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <div class="sidebar-wrap" style="width:{side}px">
    <Sidebar />
  </div>
  <div class="splitter" use:splitter={{ onStart: () => (s0 = side), onDelta: (dx) => { side = Math.max(170, Math.min(480, s0 + dx)); sideW.set(side) } }}></div>

  <div class="main">
    {#if !app.config.activeRepo}
      <div class="empty welcome">
        <div>
          <h2>Stashwick</h2>
          <p class="muted">Add a repository to the workspace on the left to get started.<br />Workspaces group related repositories; switch with the selector at the top-left.</p>
          <p class="dim small">
            {modLabel}K commit · {modLabel}Shift+K push · {modLabel}T pull · {modLabel}B branches · {modLabel}L log · {modLabel}1–9 switch repo · F5 refresh
          </p>
        </div>
      </div>
    {:else}
      <header class="toolbar row">
        <span class="repo-name ellipsis" title={app.config.activeRepo}>{repoName(app.config.activeRepo)}</span>
        <button class="branch-btn" onclick={() => (app.branchesOpen = !app.branchesOpen)} title="Branches ({modLabel}B)">
          <span class="mono">⎇ {st?.branch ?? (st?.head ? `(${short(st.head)})` : '…')}</span>
          {#if st?.ahead}<span class="ab">↑{st.ahead}</span>{/if}
          {#if st?.behind}<span class="ab">↓{st.behind}</span>{/if}
          <span class="dim">▾</span>
        </button>

        <div class="tabs row">
          <button class:on={app.view === 'commit'} onclick={() => (app.view = 'commit')} title="{modLabel}K">
            Commit {#if changeCount}<span class="badge">{changeCount}</span>{/if}
          </button>
          <button class:on={app.view === 'log'} onclick={() => (app.view = 'log')} title="{modLabel}L">Log</button>
        </div>

        <span class="grow"></span>

        <button class="flat" disabled={!!app.busy} onclick={fetchAll} title="Fetch all remotes">⇣ Fetch</button>
        <button class="flat" disabled={!!app.busy} onclick={pull} oncontextmenu={(e) => { e.preventDefault(); pullDialog() }} title="Pull ({modLabel}T) — right-click for options">⤓ Pull</button>
        <button class="flat" disabled={!!app.busy} onclick={() => pushDialog()} title="Push ({modLabel}Shift+K)">⤒ Push</button>
        <span class="vsep"></span>
        <button class="flat" disabled={!!app.busy} onclick={stashChanges} title="Stash changes">Stash</button>
        <button class="flat" onclick={() => (app.stashOpen = true)} title="Stash list">Stashes…</button>
        <button class="flat" onclick={() => (app.worktreesOpen = true)} title="List, add, open and remove worktrees">Worktrees…</button>
        <span class="vsep"></span>
        <button class="flat" onclick={refreshAll} title="Refresh (F5)">⟳</button>
        <button class="flat" onclick={() => (app.settingsOpen = true)} title="Settings">⚙</button>
      </header>

      {#if st?.state.kind}
        <div class="banner row">
          <b>{stateLabel[st.state.kind]}</b>
          <span class="muted ellipsis grow">{st.state.detail ?? ''}</span>
          {#if conflictCount}
            <span class="conf">{conflictCount} conflicted file{conflictCount > 1 ? 's' : ''}</span>
            <button class="primary" onclick={() => (app.mergeFile = st.files.find((f) => f.conflict)!.path)}>Resolve Conflicts…</button>
          {:else}
            <button class="primary" disabled={!!app.busy} onclick={() => sequencer('continue')}>Continue</button>
          {/if}
          {#if st.state.kind === 'rebasing' || st.state.kind === 'cherry-picking'}
            <button disabled={!!app.busy} onclick={() => sequencer('skip')}>Skip</button>
          {/if}
          <button disabled={!!app.busy} onclick={() => sequencer('abort')}>Abort</button>
        </div>
      {/if}

      <div class="content">
        {#if app.view === 'commit'}
          <CommitView />
        {:else}
          <LogView />
        {/if}
      </div>
    {/if}

    <footer class="statusbar row">
      {#if app.busy}<span class="spin">◐</span><span>{app.busy}…</span>{:else}<span class="dim">Ready</span>{/if}
      <span class="grow"></span>
      {#if current?.upstream}<span class="dim">tracking {current.upstream}{current.gone ? ' (gone)' : ''}</span>{/if}
      <button class="flat sb" onclick={() => (app.consoleOpen = !app.consoleOpen)}>Console{app.console.some((c) => !c.ok) ? ' •' : ''}</button>
    </footer>
  </div>
</div>

<BranchesPopup />
<MergeTool />
<Panels />
<Overlays />

<style>
  .app {
    display: flex;
    height: 100%;
  }
  .sidebar-wrap {
    flex: none;
    min-width: 0;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    height: 40px;
    flex: none;
    padding: 0 8px;
    gap: 4px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  .repo-name {
    font-weight: 600;
    font-size: 14px;
    max-width: 220px;
    padding: 0 6px;
  }
  .branch-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border-color: transparent;
    max-width: 320px;
  }
  .branch-btn:hover {
    background: var(--hover);
  }
  .branch-btn .mono {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ab {
    font-size: 11px;
    color: var(--accent);
  }
  .tabs {
    margin-left: 14px;
    gap: 0;
  }
  .tabs button {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    height: 40px;
    padding: 0 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tabs button.on {
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .tabs button:hover {
    background: var(--hover);
  }
  .vsep {
    width: 1px;
    height: 18px;
    background: var(--border2);
    margin: 0 4px;
  }
  .banner {
    padding: 6px 12px;
    background: var(--conf-bg);
    border-bottom: 1px solid var(--warn);
    gap: 8px;
    flex: none;
  }
  .conf {
    color: var(--danger);
    font-weight: 600;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .statusbar {
    height: 24px;
    flex: none;
    padding: 0 8px;
    font-size: 12px;
    border-top: 1px solid var(--border);
    background: var(--panel);
    gap: 6px;
  }
  .sb {
    font-size: 12px;
    padding: 0 6px;
  }
  .spin {
    display: inline-block;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .welcome h2 {
    margin: 0 0 8px;
    font-weight: 600;
  }
  .small {
    font-size: 12px;
  }
</style>
