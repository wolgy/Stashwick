<script lang="ts">
  import { api, type FileDiff, type FileStatus } from '../lib/api'
  import { app, op, repo, confirm, toast, showMenu, openInEditor, mod, modLabel, refreshStatus, type MenuItem } from '../lib/store.svelte'
  import { pushDialog } from '../lib/actions'
  import { splitter, sizeStore, copyText, splitPath } from '../lib/ui'
  import DiffView from './DiffView.svelte'

  type Group = 'conflicts' | 'staged' | 'changes' | 'untracked'
  interface Entry {
    group: Group
    f: FileStatus
    code: string
  }

  const leftW = sizeStore('commit.left', 380)
  let left = $state(leftW.get())
  let startLeft = 0
  const boxH = sizeStore('commit.box', 170)
  let box = $state(boxH.get())
  let startBox = 0

  let selectedKey = $state<string | null>(null)
  let multi = $state(new Set<string>())
  let anchor: string | null = null
  let diff = $state<FileDiff | null>(null)
  let collapsed = $state<Record<string, boolean>>({})

  const drafts: Record<string, string> = {}
  let message = $state('')
  let amend = $state(false)
  let signoff = $state(false)
  let lastRepo: string | null = null

  const files = $derived(app.status?.files ?? [])
  const groups = $derived.by(() => {
    const g: Record<Group, Entry[]> = { conflicts: [], staged: [], changes: [], untracked: [] }
    for (const f of files) {
      if (f.conflict) g.conflicts.push({ group: 'conflicts', f, code: 'U' })
      else if (f.untracked) g.untracked.push({ group: 'untracked', f, code: '?' })
      else {
        if (f.index !== ' ') g.staged.push({ group: 'staged', f, code: f.index })
        if (f.worktree !== ' ') g.changes.push({ group: 'changes', f, code: f.worktree })
      }
    }
    return g
  })
  const ordered = $derived(
    (['conflicts', 'staged', 'changes', 'untracked'] as Group[]).flatMap((g) => (collapsed[g] ? [] : groups[g])),
  )
  const entryKey = (e: Entry) => `${e.group}\u0000${e.f.path}`
  const selectedEntry = $derived(ordered.find((e) => entryKey(e) === selectedKey) ?? null)

  // keep commit message per repository
  $effect(() => {
    const r = app.config.activeRepo
    if (r === lastRepo) return
    if (lastRepo) drafts[lastRepo] = message
    message = (r && drafts[r]) || ''
    amend = false
    lastRepo = r
    selectedKey = null
    multi = new Set()
    diff = null
  })

  // (re)load diff for selection whenever status changes
  let diffReq = 0
  $effect(() => {
    const e = selectedEntry
    app.status
    const r = repo()
    if (!e || !r) {
      diff = null
      return
    }
    if (e.group === 'conflicts') {
      diff = { header: [], hunks: [], binary: false, tooLarge: false, untracked: false, fingerprint: '', note: 'This file has merge conflicts. Double-click it or press "Resolve" to open the merge tool.' }
      return
    }
    const id = ++diffReq
    api
      .fileDiff(r, e.f.path, e.f.origPath, e.group === 'staged', e.group === 'untracked')
      .then((d) => {
        if (id !== diffReq) return
        // skip identical reloads (background polling) so the view doesn't flicker
        if (diff && d.fingerprint && d.fingerprint === diff.fingerprint && d.hunks.length === diff.hunks.length) return
        diff = d
      })
      .catch((err) => id === diffReq && (diff = { header: [], hunks: [], binary: false, tooLarge: false, untracked: false, fingerprint: '', note: String(err) }))
  })

  function click(e: Entry, ev: MouseEvent) {
    const k = entryKey(e)
    if (ev.shiftKey && anchor) {
      const keys = ordered.map(entryKey)
      const [a, b] = [keys.indexOf(anchor), keys.indexOf(k)].sort((x, y) => x - y)
      multi = new Set(keys.slice(a, b + 1))
    } else if (mod(ev)) {
      const n = new Set(multi)
      n.has(k) ? n.delete(k) : n.add(k)
      multi = n
      anchor = k
    } else {
      multi = new Set([k])
      anchor = k
    }
    selectedKey = k
  }

  function targets(e: Entry): Entry[] {
    const k = entryKey(e)
    if (multi.has(k)) return ordered.filter((x) => multi.has(entryKey(x)))
    return [e]
  }

  const pathsOf = (es: Entry[]) => [...new Set(es.flatMap((e) => (e.f.origPath && e.group === 'staged' ? [e.f.path, e.f.origPath] : [e.f.path])))]

  async function stage(es: Entry[]) {
    if (!es.length) return
    await op(`Stage ${es.length} file(s)`, (r) => api.stage(r, pathsOf(es)))
  }
  async function unstage(es: Entry[]) {
    if (!es.length) return
    await op(`Unstage ${es.length} file(s)`, (r) => api.unstage(r, pathsOf(es)))
  }
  async function rollback(es: Entry[]) {
    if (!es.length) return
    const fs = [...new Map(es.map((e) => [e.f.path, e.f])).values()]
    const deleting = fs.filter((f) => f.untracked || f.index === 'A')
    const msg =
      `Roll back ${fs.length} file(s) to HEAD? Local changes (staged and unstaged) will be lost.` +
      (deleting.length ? `\n\n${deleting.length} new file(s) will be deleted from disk.` : '')
    if (!(await confirm('Rollback Changes', msg, 'Rollback', true, fs.map((f) => f.path).join('\n')))) return
    await op('Rollback', (r) => api.rollback(r, fs))
  }

  async function applyLines(action: 'stage' | 'unstage' | 'discard', sel: { hunk: number; line: number }[]) {
    const e = selectedEntry
    if (!e || !diff || !sel.length) return
    if (action === 'discard' && !(await confirm('Rollback Lines', `Discard ${sel.length} changed line(s) from the working tree? This cannot be undone.`, 'Rollback', true))) return
    const label = action === 'stage' ? 'Stage lines' : action === 'unstage' ? 'Unstage lines' : 'Rollback lines'
    await op(label, (r) => api.applyLines(r, e.f.path, e.f.origPath, action, diff!.fingerprint, sel))
  }

  function menu(e: Entry, ev: MouseEvent) {
    if (!multi.has(entryKey(e))) click(e, ev)
    const ts = targets(e)
    const items: (MenuItem | '-')[] = []
    if (e.group === 'conflicts') {
      items.push(
        { label: 'Resolve Conflict…', action: () => (app.mergeFile = e.f.path) },
        { label: `Accept ${app.status?.state.oursLabel ?? 'Yours'}`, action: () => op('Accept ours', (r) => api.acceptSide(r, e.f.path, 'ours')) },
        { label: `Accept ${app.status?.state.theirsLabel ?? 'Theirs'}`, action: () => op('Accept theirs', (r) => api.acceptSide(r, e.f.path, 'theirs')) },
        { label: 'Mark as Resolved (stage as-is)', action: () => stage(ts) },
        '-',
      )
    } else {
      const canStage = ts.some((t) => t.group !== 'staged')
      const canUnstage = ts.some((t) => t.group === 'staged')
      if (canStage) items.push({ label: 'Stage', action: () => stage(ts.filter((t) => t.group !== 'staged')) })
      if (canUnstage) items.push({ label: 'Unstage', action: () => unstage(ts.filter((t) => t.group === 'staged')) })
      items.push({ label: 'Rollback…', danger: true, action: () => rollback(ts) }, '-')
    }
    items.push(
      { label: 'Open in Editor', action: () => openInEditor(ts.map((t) => t.f.path)) },
      { label: 'Show History', disabled: e.group === 'untracked', action: () => { app.logFilter = { branch: null, path: e.f.path }; app.view = 'log' } },
      { label: 'Copy Path', action: () => copyText(ts.map((t) => t.f.path).join('\n')) },
    )
    showMenu(ev, items)
  }

  function groupMenu(g: Group, ev: MouseEvent) {
    const es = groups[g]
    const items: (MenuItem | '-')[] = []
    if (g === 'staged') items.push({ label: 'Unstage All', action: () => unstage(es) })
    else if (g !== 'conflicts') items.push({ label: 'Stage All', action: () => stage(es) }, { label: 'Rollback All…', danger: true, action: () => rollback(es) })
    showMenu(ev, items)
  }

  function onAmend() {
    if (amend && !message.trim() && repo()) {
      api.lastMessage(repo()!).then((m) => {
        if (amend && !message.trim()) message = m
      })
    }
  }

  async function commit(andPush = false) {
    const st = app.status
    if (!st) return
    if (groups.conflicts.length) return toast('error', 'Resolve all conflicts before committing.')
    if (!groups.staged.length && !amend) {
      const all = [...groups.changes, ...groups.untracked]
      if (!all.length && !st.state.kind) return toast('info', 'Nothing to commit.')
      if (all.length) {
        if (!(await confirm('Nothing Staged', `No changes are staged. Stage all ${all.length} changed file(s) and commit?`, 'Stage All & Commit'))) return
        const ok = await op('Stage all', (r) => api.stage(r, pathsOf(all)))
        if (ok === undefined) return
      }
    }
    if (!message.trim() && !amend && st.state.kind !== 'merging') {
      toast('error', 'Please enter a commit message.')
      return
    }
    const msg = message
    const res = await op(amend ? 'Amend commit' : 'Commit', (r) =>
      !msg.trim() && st.state.kind === 'merging' ? api.sequencer(r, 'merge', 'continue') : api.commit(r, msg, amend, signoff),
    )
    if (res === undefined) return
    message = ''
    if (repo()) drafts[repo()!] = ''
    const wasAmend = amend
    amend = false
    toast('success', wasAmend ? 'Commit amended' : 'Committed')
    if (andPush) await pushDialog(null, wasAmend)
  }

  function onMsgKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && mod(e)) {
      e.preventDefault()
      commit(e.shiftKey)
    }
  }

  function fileKey(e: KeyboardEvent) {
    if (!['ArrowDown', 'ArrowUp'].includes(e.key) && e.key !== ' ' && e.key !== 'Enter') return
    e.preventDefault()
    const keys = ordered.map(entryKey)
    const i = selectedKey ? keys.indexOf(selectedKey) : -1
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const n = keys[Math.max(0, Math.min(keys.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))]
      if (n) {
        selectedKey = n
        multi = new Set([n])
        anchor = n
        document.querySelector(`[data-key="${CSS.escape(n)}"]`)?.scrollIntoView({ block: 'nearest' })
      }
    } else if (e.key === ' ' && selectedEntry) {
      const ts = targets(selectedEntry)
      ts.every((t) => t.group === 'staged') ? unstage(ts) : stage(ts.filter((t) => t.group !== 'staged'))
    } else if (e.key === 'Enter' && selectedEntry) {
      selectedEntry.group === 'conflicts' ? (app.mergeFile = selectedEntry.f.path) : openInEditor([selectedEntry.f.path])
    }
  }

  const titles: Record<Group, string> = { conflicts: 'Merge Conflicts', staged: 'Staged', changes: 'Changes', untracked: 'Unversioned Files' }
</script>

<div class="commitview">
  <div class="left" style="width:{left}px">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="files" tabindex="0" onkeydown={fileKey} role="listbox">
      <div class="toolbar row">
        <button class="icon" title="Refresh (F5)" onclick={refreshStatus}>⟳</button>
        <button class="icon" title="Stage all" disabled={!groups.changes.length && !groups.untracked.length} onclick={() => stage([...groups.changes, ...groups.untracked])}>＋ Stage All</button>
        <button class="icon" title="Unstage all" disabled={!groups.staged.length} onclick={() => unstage(groups.staged)}>－ Unstage All</button>
        <span class="grow"></span>
        <span class="dim small">{files.length} file{files.length === 1 ? '' : 's'}</span>
      </div>
      {#if !app.status}
        <div class="empty">Loading…</div>
      {:else if !files.length}
        <div class="empty">
          <div>
            <div style="font-size:28px">✓</div>
            Working tree clean
          </div>
        </div>
      {/if}
      {#each ['conflicts', 'staged', 'changes', 'untracked'] as const as g}
        {#if groups[g].length}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="group-head row" class:conflict={g === 'conflicts'} onclick={() => (collapsed[g] = !collapsed[g])} oncontextmenu={(e) => groupMenu(g, e)}>
            <span class="caret">{collapsed[g] ? '▸' : '▾'}</span>
            <span class="grow">{titles[g]} <span class="dim">{groups[g].length}</span></span>
            {#if g === 'staged'}
              <button class="icon small" title="Unstage all" onclick={(e) => { e.stopPropagation(); unstage(groups.staged) }}>－</button>
            {:else if g !== 'conflicts'}
              <button class="icon small" title="Stage all" onclick={(e) => { e.stopPropagation(); stage(groups[g]) }}>＋</button>
            {/if}
          </div>
          {#if !collapsed[g]}
            {#each groups[g] as e (entryKey(e))}
              {@const k = entryKey(e)}
              {@const sp = splitPath(e.f.path)}
              {@const cls = e.code === '?' ? 'Q' : e.code}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div
                class="file row"
                class:sel={multi.has(k)}
                class:focus={selectedKey === k}
                data-key={k}
                onclick={(ev) => click(e, ev)}
                ondblclick={() => (e.group === 'conflicts' ? (app.mergeFile = e.f.path) : openInEditor([e.f.path]))}
                oncontextmenu={(ev) => menu(e, ev)}
                title={e.f.origPath ? `${e.f.origPath} → ${e.f.path}` : e.f.path}
              >
                <span class="st st-{cls}">{e.code === '?' ? 'U' : e.code}</span>
                <span class="fname fname-{cls}">{sp.name}</span>
                <span class="dir dim ellipsis grow">{e.f.origPath && e.group === 'staged' ? `← ${e.f.origPath}` : sp.dir}</span>
                <span class="acts">
                  {#if e.group === 'conflicts'}
                    <button class="mini primary" onclick={(ev) => { ev.stopPropagation(); app.mergeFile = e.f.path }}>Resolve</button>
                  {:else if e.group === 'staged'}
                    <button class="mini" title="Unstage" onclick={(ev) => { ev.stopPropagation(); unstage(targets(e)) }}>－</button>
                  {:else}
                    <button class="mini" title="Rollback" onclick={(ev) => { ev.stopPropagation(); rollback(targets(e)) }}>↺</button>
                    <button class="mini" title="Stage" onclick={(ev) => { ev.stopPropagation(); stage(targets(e)) }}>＋</button>
                  {/if}
                </span>
              </div>
            {/each}
          {/if}
        {/if}
      {/each}
    </div>

    <div class="splitter h" use:splitter={{ onStart: () => (startBox = box), onDelta: (_, dy) => { box = Math.max(110, Math.min(500, startBox - dy)); boxH.set(box) } }}></div>

    <div class="commitbox" style="height:{box}px">
      <textarea
        bind:value={message}
        onkeydown={onMsgKey}
        placeholder={app.status?.state.kind === 'merging' ? 'Merge commit message (leave empty for the default)' : 'Commit message'}
        spellcheck="true"
      ></textarea>
      <div class="row opts">
        <label class="row"><input type="checkbox" bind:checked={amend} onchange={onAmend} /> Amend</label>
        <label class="row"><input type="checkbox" bind:checked={signoff} /> Sign-off</label>
        <span class="grow"></span>
        <span class="dim small">{groups.staged.length} staged</span>
      </div>
      <div class="row">
        <button class="primary grow" disabled={!!app.busy} onclick={() => commit(false)} title="{modLabel}Enter">{amend ? 'Amend Commit' : 'Commit'}</button>
        <button disabled={!!app.busy} onclick={() => commit(true)} title="{modLabel}Shift+Enter">{amend ? 'Amend' : 'Commit'} and Push…</button>
      </div>
    </div>
  </div>

  <div class="splitter" use:splitter={{ onStart: () => (startLeft = left), onDelta: (dx) => { left = Math.max(260, Math.min(900, startLeft + dx)); leftW.set(left) } }}></div>

  <div class="right">
    <DiffView
      {diff}
      mode={selectedEntry?.group === 'staged' ? 'staged' : selectedEntry?.group === 'changes' || selectedEntry?.group === 'untracked' ? 'unstaged' : 'readonly'}
      title={selectedEntry ? `${selectedEntry.f.path}  ·  ${selectedEntry.group === 'staged' ? 'Staged (HEAD ↔ Index)' : selectedEntry.group === 'untracked' ? 'Untracked' : selectedEntry.group === 'conflicts' ? 'Conflict' : 'Unstaged (Index ↔ Working tree)'}` : ''}
      onApply={applyLines}
    />
  </div>
</div>

<style>
  .commitview {
    display: flex;
    height: 100%;
    min-height: 0;
  }
  .left {
    display: flex;
    flex-direction: column;
    flex: none;
    min-width: 0;
    background: var(--panel);
  }
  .right {
    flex: 1;
    min-width: 0;
  }
  .files {
    flex: 1;
    overflow-y: auto;
    outline: none;
    min-height: 0;
  }
  .toolbar {
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    gap: 2px;
    position: sticky;
    top: 0;
    background: var(--panel);
    z-index: 1;
  }
  .toolbar .icon {
    font-size: 12px;
  }
  .small {
    font-size: 11.5px;
  }
  .group-head {
    padding: 5px 8px 3px;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
  }
  .group-head:hover {
    background: var(--hover);
  }
  .group-head.conflict {
    color: var(--danger);
  }
  .caret {
    width: 10px;
    color: var(--fg3);
  }
  .group-head .icon {
    padding: 0 6px;
  }
  .file {
    padding: 2px 8px 2px 24px;
    gap: 6px;
    height: 24px;
    white-space: nowrap;
  }
  .file:hover {
    background: var(--hover);
  }
  .file.sel {
    background: var(--sel-inactive);
  }
  .files:focus-within .file.sel {
    background: var(--sel);
  }
  .fname {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dir {
    font-size: 11.5px;
  }
  .acts {
    display: none;
    gap: 2px;
  }
  .file:hover .acts {
    display: flex;
  }
  .mini {
    padding: 0 6px;
    font-size: 11px;
    line-height: 17px;
  }
  .commitbox {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    flex: none;
  }
  .commitbox textarea {
    flex: 1;
    min-height: 40px;
  }
  .opts {
    gap: 12px;
    font-size: 12px;
  }
  .opts label {
    gap: 4px;
  }
</style>
