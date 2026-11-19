<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog'
  import {
    app, workspace, repoName, selectRepo, selectWorkspace, addRepoPaths, removeRepo, newWorkspace,
    saveConfig, showMenu, promptText, confirm, toast, openInEditor, modLabel, dialog,
  } from '../lib/store.svelte'
  import { api } from '../lib/api'

  let filter = $state('')
  const ws = $derived(workspace())
  const repos = $derived(ws.repos.filter((r) => !filter || repoName(r).toLowerCase().includes(filter.toLowerCase())))

  async function addRepo() {
    const sel = await open({ directory: true, multiple: true, title: 'Add Git repositories' })
    if (!sel) return
    await addRepoPaths(Array.isArray(sel) ? sel : [sel])
  }

  async function scanFolder() {
    const dir = await open({ directory: true, title: 'Scan a folder for Git repositories' })
    if (!dir || Array.isArray(dir)) return
    const found = await api.scanRepos(dir, 3)
    if (!found.length) return toast('info', 'No Git repositories found (searched 3 levels deep).')
    const r = await pickRepos(found)
    if (r?.length) await addRepoPaths(r)
  }

  async function pickRepos(found: string[]) {
    const res = await dialog({
      title: `Found ${found.length} repositories`,
      message: 'Select the repositories to add to this workspace:',
      fields: found.map((f, i) => ({ key: String(i), label: f, type: 'checkbox' as const, value: !ws.repos.includes(f) })),
      ok: 'Add selected',
    })
    return res ? found.filter((_, i) => res[String(i)]) : null
  }

  function wsMenu(e: MouseEvent) {
    showMenu(e, [
      ...app.config.workspaces.map((w) => ({
        label: (w.id === ws.id ? '✓ ' : '   ') + `${w.name}  (${w.repos.length})`,
        action: () => selectWorkspace(w.id),
      })),
      '-',
      {
        label: 'New Workspace…',
        action: async () => {
          const n = await promptText('New Workspace', 'Name')
          if (n) newWorkspace(n)
        },
      },
      {
        label: 'Rename Workspace…',
        action: async () => {
          const n = await promptText('Rename Workspace', 'Name', ws.name)
          if (n) {
            ws.name = n
            saveConfig()
          }
        },
      },
      {
        label: 'Delete Workspace',
        danger: true,
        disabled: app.config.workspaces.length < 2,
        action: async () => {
          if (!(await confirm('Delete Workspace', `Delete workspace "${ws.name}"? Repositories on disk are not touched.`, 'Delete', true))) return
          app.config.workspaces = app.config.workspaces.filter((w) => w.id !== ws.id)
          selectWorkspace(app.config.workspaces[0].id)
        },
      },
    ])
  }

  function repoMenu(e: MouseEvent, path: string) {
    const others = app.config.workspaces.filter((w) => w.id !== ws.id)
    showMenu(e, [
      { label: 'Open in Editor', action: () => { selectRepo(path); openInEditor([path]) } },
      { label: 'Copy Path', action: () => navigator.clipboard.writeText(path) },
      { label: 'Worktrees…', action: () => { selectRepo(path); app.worktreesOpen = true } },
      '-',
      ...others.map((w) => ({
        label: `Move to "${w.name}"`,
        action: () => {
          if (!w.repos.includes(path)) w.repos.push(path)
          removeRepo(path)
        },
      })),
      ...others.map((w) => ({
        label: `Also add to "${w.name}"`,
        action: () => {
          if (!w.repos.includes(path)) w.repos.push(path)
          saveConfig()
        },
      })),
      ...(others.length ? ['-' as const] : []),
      { label: 'Move Up', disabled: ws.repos.indexOf(path) === 0, action: () => move(path, -1) },
      { label: 'Move Down', disabled: ws.repos.indexOf(path) === ws.repos.length - 1, action: () => move(path, 1) },
      '-',
      { label: 'Remove from Workspace', danger: true, action: () => removeRepo(path) },
    ])
  }

  function move(path: string, d: number) {
    const i = ws.repos.indexOf(path)
    const j = i + d
    if (j < 0 || j >= ws.repos.length) return
    ;[ws.repos[i], ws.repos[j]] = [ws.repos[j], ws.repos[i]]
    saveConfig()
  }
</script>

<aside class="sidebar">
  <button class="ws flat" onclick={wsMenu} title="Switch workspace">
    <span class="ws-label dim">WORKSPACE</span>
    <span class="row"><span class="grow ellipsis ws-name">{ws.name}</span><span class="dim">▾</span></span>
  </button>

  <div class="search">
    <input type="text" placeholder="Filter repositories" bind:value={filter} spellcheck="false" />
  </div>

  <div class="list">
    {#each repos as path, i (path)}
      {@const s = app.summaries[path]}
      <button
        class="repo"
        class:active={app.config.activeRepo === path}
        onclick={() => selectRepo(path)}
        oncontextmenu={(e) => repoMenu(e, path)}
        title={path + (i < 9 ? `\n${modLabel}${i + 1}` : '')}
      >
        <div class="row">
          <span class="name grow ellipsis">{repoName(path)}</span>
          {#if s?.linked}<span class="wt" title="Linked worktree">⧉</span>{/if}
          {#if s?.conflicts}<span class="badge conflict" title="Conflicts">!{s.conflicts}</span>
          {:else if s?.changes}<span class="badge" title="Changed files">{s.changes}</span>{/if}
        </div>
        <div class="row sub">
          {#if s?.error}
            <span class="err ellipsis grow">⚠ {s.error}</span>
          {:else}
            <span class="branch ellipsis grow">⎇ {s?.branch ?? '…'}</span>
            {#if s?.state}<span class="state">{s.state}</span>{/if}
            {#if s?.ahead}<span class="ab" title="Ahead">↑{s.ahead}</span>{/if}
            {#if s?.behind}<span class="ab" title="Behind">↓{s.behind}</span>{/if}
          {/if}
        </div>
      </button>
    {:else}
      <div class="hint dim">
        {#if filter}No match.{:else}No repositories in this workspace yet.{/if}
      </div>
    {/each}
  </div>

  <div class="actions">
    <button onclick={addRepo}>+ Add Repository</button>
    <button class="flat" onclick={scanFolder} title="Find all repositories inside a folder">Scan Folder…</button>
  </div>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--panel);
    min-width: 0;
  }
  .ws {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    text-align: left;
    margin: 6px;
    padding: 6px 8px;
    border-radius: 6px;
  }
  .ws-label {
    font-size: 10px;
    letter-spacing: 0.06em;
  }
  .ws-name {
    font-weight: 600;
    font-size: 14px;
  }
  .search {
    padding: 0 8px 6px;
  }
  .search input {
    width: 100%;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px;
  }
  .repo {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-radius: 6px;
    padding: 6px 8px;
    margin-bottom: 2px;
  }
  .repo:hover {
    background: var(--hover);
  }
  .repo.active {
    background: var(--sel);
  }
  .name {
    font-weight: 500;
  }
  .sub {
    font-size: 11.5px;
    color: var(--fg2);
    margin-top: 1px;
  }
  .branch {
    font-family: var(--mono);
    font-size: 11px;
  }
  .wt {
    color: var(--accent);
    font-size: 12px;
  }
  .ab {
    font-size: 11px;
    color: var(--accent);
  }
  .state {
    font-size: 10px;
    color: var(--warn);
    text-transform: uppercase;
  }
  .err {
    color: var(--danger);
  }
  .badge.conflict {
    background: var(--danger);
    color: #fff;
    border-color: var(--danger);
  }
  .hint {
    padding: 14px 8px;
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-top: 1px solid var(--border);
    flex-wrap: wrap;
  }
</style>
