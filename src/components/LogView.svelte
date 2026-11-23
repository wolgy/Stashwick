<script lang="ts">
  import { api, type BranchInfo, type Commit, type CommitDetail, type FileDiff, type ChangedFile } from '../lib/api'
  import { app, repo, op, fmtDate, short, showMenu, toast, mod, saveConfig, type MenuItem } from '../lib/store.svelte'
  import { layoutGraph, newGraphState, laneColor, MAIN, type GraphRow, type GraphState } from '../lib/graph'
  import { branchMenuItems, checkout, newBranch, newTag, resetTo, mergeInto, rebaseOnto, newWorktree, otherWorktree, squashCommits } from '../lib/actions'
  import { splitter, sizeStore, copyText, splitPath } from '../lib/ui'
  import DiffView from './DiffView.svelte'
  import { fileTree, type TreeRow } from '../lib/filetree'

  const ROW = 24
  const LANE = 14

  const treeW = sizeStore('log.tree', 220)
  const detW = sizeStore('log.details', 330)
  let tree = $state(treeW.get())
  let details = $state(detW.get())
  let s0 = 0

  // raw: these lists get large and are only ever replaced, never mutated in place
  let commits = $state.raw<Commit[]>([])
  let graph = $state.raw<GraphRow[]>([])
  let graphState: GraphState = newGraphState()
  let exhausted = $state(false)
  let loading = $state(false)
  let error = $state<string | null>(null)
  let selected = $state<string | null>(null)
  // multi-selection (⌘/Ctrl-click, Shift-click); `selected` is the one shown in the details panel
  let marked = $state<string[]>([])
  let anchor: string | null = null
  const markedSet = $derived(new Set(marked))
  let detail = $state<CommitDetail | null>(null)
  let author = $state('')
  let text = $state('')
  let treeFilter = $state('')
  let collapsed = $state<Record<string, boolean>>({ tags: true })

  let scroller = $state<HTMLDivElement>()
  let scrollTop = $state(0)
  let viewH = $state(600)

  let diffFile = $state<ChangedFile | null>(null)
  let collapsedDirs = $state<Record<string, boolean>>({})
  const grouped = $derived(app.config.settings.groupByDir)
  const fileRows = $derived.by((): TreeRow<ChangedFile>[] => {
    const fs = detail?.files ?? []
    return grouped ? fileTree(fs, (f) => f.path, (k) => !!collapsedDirs[k]) : fs.map((f) => ({ kind: 'file', key: f.path, item: f, depth: 0 }))
  })
  // ◀ ▶ in the diff window follow the order on screen
  const visibleFiles = $derived(fileRows.flatMap((r) => (r.kind === 'file' ? [r.item] : [])))
  function toggleGrouping() {
    app.config.settings.groupByDir = !grouped
    saveConfig()
  }
  let fileDiff = $state<FileDiff | null>(null)

  // ───── column widths ─────
  // 0 = auto: fit the content of the loaded commits. Dragging a header edge pins a width; double-click returns to auto.
  type Col = 'author' | 'date' | 'hash'
  const COLS: Col[] = ['author', 'date', 'hash']
  const COL_MIN = 36
  const COL_MAX: Record<Col, number> = { author: 320, date: 260, hash: 160 }
  const colStores = { author: sizeStore('log.col.author', 0), date: sizeStore('log.col.date', 0), hash: sizeStore('log.col.hash', 0) }
  let colW = $state<Record<Col, number>>({ author: colStores.author.get(), date: colStores.date.get(), hash: colStores.hash.get() })

  // text measuring via canvas (no DOM layout); results cached per string so reloads only measure new values
  let fonts = $state({ ui: '', mono: '' })
  const ctxs: Record<string, CanvasRenderingContext2D | null> = {}
  const textW = new Map<string, number>()
  function measure(s: string, kind: 'ui' | 'mono' = 'ui') {
    const key = kind + '\0' + s
    let w = textW.get(key)
    if (w === undefined) {
      let ctx = ctxs[kind]
      if (!ctx) {
        ctx = ctxs[kind] = document.createElement('canvas').getContext('2d')
        if (!ctx) return s.length * 7
        ctx.font = fonts[kind]
      }
      w = Math.ceil(ctx.measureText(s).width)
      textW.set(key, w)
    }
    return w
  }
  $effect(() => {
    if (!scroller) return
    const cs = getComputedStyle(scroller)
    const f = { ui: cs.font, mono: `12px ${cs.getPropertyValue('--mono')}` }
    if (f.ui !== fonts.ui || f.mono !== fonts.mono) {
      ctxs.ui = ctxs.mono = null
      textW.clear()
      fonts = f
    }
  })

  const autoW = $derived.by((): Record<Col, number> => {
    if (!fonts.ui) return { author: 130, date: 140, hash: 70 } // wait for the real font
    let a = 0
    let d = 0
    const seen = new Set<string>()
    for (const c of commits) {
      if (seen.has(c.author)) continue
      seen.add(c.author)
      a = Math.max(a, measure(c.author))
    }
    // dates share a handful of formats: a sample is enough and avoids formatting thousands of timestamps
    const step = Math.max(1, Math.floor(commits.length / 400))
    for (let i = 0; i < commits.length; i += step) d = Math.max(d, measure(fmtDate(commits[i].time)))
    const h = commits.length ? measure(short(commits[0].hash), 'mono') : 0
    const fit = (v: number, col: Col, fallback: number) => (v ? Math.min(COL_MAX[col], Math.max(COL_MIN, v + 6)) : fallback)
    return { author: fit(a, 'author', 130), date: fit(d, 'date', 140), hash: fit(h, 'hash', 70) }
  })
  const colWidth = (col: Col) => colW[col] || autoW[col]

  // Fit everything into the panel so nothing overlaps: the graph may take at most 40% of the room left after
  // the subject's minimum, and the fixed columns shrink proportionally (and ellipsize) when they don't fit.
  const SUBJ_MIN = 90 // keep in sync with .subject min-width
  const ROW_CHROME = 5 * 6 + 8 // 5 flex gaps + right padding
  let centerW = $state(0)
  const layout = $derived.by(() => {
    const room = Math.max(0, centerW - ROW_CHROME - SUBJ_MIN)
    const graphShown = centerW ? Math.min(graphW, Math.max(40, Math.floor(room * 0.4))) : graphW
    const want = COLS.map(colWidth)
    const total = want.reduce((a, b) => a + b, 0)
    const budget = room - graphShown
    const k = centerW && total > budget ? Math.max(0, budget) / total : 1
    const cols = Object.fromEntries(COLS.map((c, i) => [c, Math.max(24, Math.floor(want[i] * k))])) as Record<Col, number>
    return { graph: graphShown, cols }
  })
  const colStyle = $derived(COLS.map((col) => `--w-${col}:${layout.cols[col]}px`).join(';'))

  let c0 = 0
  function colDrag(col: Col) {
    return {
      onStart: () => (c0 = layout.cols[col]),
      // the handle sits on the column's left edge: dragging left widens it
      onDelta: (dx: number) => {
        colW[col] = Math.round(Math.max(COL_MIN, Math.min(600, c0 - dx)))
        colStores[col].set(colW[col])
      },
    }
  }
  function colAuto(col: Col) {
    colW[col] = 0
    colStores[col].set(0)
  }

  const filtered = $derived(!!(author.trim() || text.trim() || app.logFilter.path.trim()))
  const graphW = $derived(filtered ? 18 : Math.min(Math.max(...graph.slice(0, 2000).map((g) => g.width), 1) * LANE + 8, 260))

  // ───── loading ─────
  let reqId = 0
  let keepLoaded = false
  async function load(reset: boolean) {
    const r = repo()
    if (!r) return
    const id = ++reqId
    // a background reload keeps as many rows as are already loaded, so the scroll position stays valid
    const pageSize = app.config.settings.logPageSize
    const limit = reset && keepLoaded ? Math.max(pageSize, commits.length) : pageSize
    keepLoaded = false
    loading = true
    error = null
    try {
      const page = await api.log(r, {
        branch: app.logFilter.branch,
        author: author || null,
        text: text || null,
        path: app.logFilter.path || null,
        skip: reset ? 0 : commits.length,
        limit,
      })
      if (id !== reqId) return
      if (reset) graphState = newGraphState()
      const rows = layoutGraph(page, graphState)
      commits = reset ? page : [...commits, ...page]
      graph = reset ? rows : [...graph, ...rows]
      exhausted = page.length < limit
      if (reset) {
        const have = new Set(commits.map((c) => c.hash))
        if (selected && !have.has(selected)) selected = null
        if (marked.some((h) => !have.has(h))) marked = marked.filter((h) => have.has(h))
      }
      if (reset && !selected && commits.length) select(commits[0].hash)
    } catch (e) {
      if (id === reqId) {
        error = String(e)
        commits = []
        graph = []
      }
    } finally {
      if (id === reqId) loading = false
    }
  }

  let debounce: ReturnType<typeof setTimeout>
  let seenRefs = -1
  $effect(() => {
    // dependencies
    const refs = app.refsToken
    app.config.activeRepo
    app.logFilter.branch
    app.logFilter.path
    author
    text
    // only the refs changed (commit, fetch…): reload in place; otherwise start from the top
    const refsOnly = seenRefs !== -1 && refs !== seenRefs
    seenRefs = refs
    clearTimeout(debounce)
    debounce = setTimeout(() => {
      keepLoaded = refsOnly
      load(true)
    }, 200)
  })

  let detailReq = 0
  async function select(hash: string, keepMarked = false) {
    selected = hash
    if (!keepMarked) {
      marked = [hash]
      anchor = hash
    }
    const r = repo()
    if (!r) return
    const id = ++detailReq
    try {
      const d = await api.commitDetail(r, hash)
      if (id === detailReq) detail = d
    } catch (e) {
      if (id === detailReq) detail = null
    }
  }

  function clickRow(c: Commit, idx: number, e: MouseEvent) {
    if (mod(e)) {
      if (markedSet.has(c.hash) && marked.length > 1) {
        marked = marked.filter((h) => h !== c.hash)
        if (selected === c.hash) select(marked[marked.length - 1], true)
      } else {
        marked = [...marked, c.hash]
        select(c.hash, true)
      }
      anchor = c.hash
    } else if (e.shiftKey && anchor) {
      const a = commits.findIndex((x) => x.hash === anchor)
      if (a < 0) return select(c.hash)
      marked = commits.slice(Math.min(a, idx), Math.max(a, idx) + 1).map((x) => x.hash)
      select(c.hash, true)
    } else select(c.hash)
  }

  async function openFile(f: ChangedFile) {
    const r = repo()
    if (!r || !detail) return
    diffFile = f
    fileDiff = null
    try {
      fileDiff = await api.commitFileDiff(r, detail.hash, f.path, f.origPath)
    } catch (e) {
      toast('error', String(e))
    }
  }
  function stepFile(d: number) {
    if (!detail || !diffFile) return
    const i = visibleFiles.findIndex((f) => f.path === diffFile!.path)
    const n = visibleFiles[i + d]
    if (n) openFile(n)
  }

  // ───── virtual list ─────
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10))
  const end = $derived(Math.min(commits.length, Math.ceil((scrollTop + viewH) / ROW) + 10))
  function onScroll() {
    scrollTop = scroller!.scrollTop
    if (!exhausted && !loading && scrollTop + viewH > commits.length * ROW - ROW * 30) load(false)
  }

  function onKey(e: KeyboardEvent) {
    if (!['ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', 'Home', 'End'].includes(e.key)) return
    e.preventDefault()
    const i = commits.findIndex((c) => c.hash === selected)
    const page = Math.floor(viewH / ROW)
    const delta = { ArrowDown: 1, ArrowUp: -1, PageDown: page, PageUp: -page, Home: -1e9, End: 1e9 }[e.key]!
    const n = Math.max(0, Math.min(commits.length - 1, i + delta))
    if (commits[n]) {
      select(commits[n].hash)
      const top = n * ROW
      if (top < scroller!.scrollTop) scroller!.scrollTop = top
      else if (top + ROW > scroller!.scrollTop + viewH) scroller!.scrollTop = top + ROW - viewH
    }
  }

  // ───── graph drawing ─────
  const x = (lane: number) => lane * LANE + LANE / 2 + 4
  function curve(x1: number, y1: number, x2: number, y2: number) {
    if (x1 === x2) return `M${x1} ${y1}L${x2} ${y2}`
    const my = (y1 + y2) / 2
    return `M${x1} ${y1}C${x1} ${my} ${x2} ${my} ${x2} ${y2}`
  }

  // ───── menus ─────
  function commitMenu(c: Commit, e: MouseEvent) {
    if (marked.length > 1 && markedSet.has(c.hash)) return multiMenu(e)
    select(c.hash)
    const local = c.refs.find((r) => r.kind === 'local' && !r.current)
    const target = local?.name ?? c.hash
    const label = local?.name ?? short(c.hash)
    const items: (MenuItem | '-')[] = [
      { label: 'Copy Revision Number', action: () => copyText(c.hash) },
      { label: 'Copy Subject', action: () => copyText(c.subject) },
      '-',
      { label: 'Checkout Revision', action: () => checkout({ name: c.hash, kind: 'commit' }) },
      ...(local ? [{ label: `Checkout '${local.name}'`, action: () => checkout(app.branches.find((b) => b.kind === 'local' && b.name === local.name) ?? { name: local.name, kind: 'local' }) }] : []),
      { label: 'New Branch…', action: () => newBranch(c.hash, short(c.hash)) },
      { label: 'New Worktree at This Revision…', action: () => newWorktree({ name: c.hash, kind: 'commit' }) },
      { label: 'New Tag…', action: () => newTag(c.hash, short(c.hash)) },
      '-',
      { label: 'Cherry-Pick', action: () => op(`Cherry-pick ${short(c.hash)}`, (r) => api.cherryPick(r, c.hash)) },
      { label: 'Revert Commit', action: () => op(`Revert ${short(c.hash)}`, (r) => api.revert(r, c.hash)) },
      { label: `Reset Current Branch to Here…`, danger: true, action: () => resetTo(c.hash) },
      '-',
      { label: `Merge '${label}' into Current`, action: () => mergeInto(target) },
      { label: `Rebase Current onto '${label}'`, action: () => rebaseOnto(target) },
    ]
    showMenu(e, items)
  }

  function multiMenu(e: MouseEvent) {
    const list = commits.filter((c) => markedSet.has(c.hash)) // newest first
    const merges = list.some((c) => c.parents.length > 1)
    showMenu(e, [
      { label: 'Copy Revision Numbers', action: () => copyText(list.map((c) => c.hash).join('\n')) },
      '-',
      { label: `Squash ${list.length} Commits…${merges ? ' (no merge commits)' : ''}`, disabled: merges, action: () => squashCommits(list) },
    ])
  }

  // ───── branch tree ─────
  const tf = $derived(treeFilter.toLowerCase())
  const locals = $derived(app.branches.filter((b) => b.kind === 'local' && b.name.toLowerCase().includes(tf)))
  const remotesByName = $derived.by(() => {
    const m: Record<string, BranchInfo[]> = {}
    for (const b of app.branches) {
      if (b.kind !== 'remote' || !b.name.toLowerCase().includes(tf)) continue
      const r = b.name.split('/')[0]
      ;(m[r] ??= []).push(b)
    }
    return m
  })
  const tags = $derived(app.branches.filter((b) => b.kind === 'tag' && b.name.toLowerCase().includes(tf)))

  function setBranch(b: string | null) {
    app.logFilter.branch = b
  }
  function treeMenu(b: BranchInfo, e: MouseEvent) {
    showMenu(e, branchMenuItems(b, ['-', { label: 'Show in Log Only', action: () => setBranch(b.name) }]))
  }

  const statusName: Record<string, string> = { A: 'Added', M: 'Modified', D: 'Deleted', R: 'Renamed', C: 'Copied', T: 'Type changed' }
</script>

<div class="logview">
  <!-- branch tree -->
  <div class="tree" style="width:{tree}px">
    <div class="tree-search"><input type="text" placeholder="Filter branches" bind:value={treeFilter} spellcheck="false" /></div>
    <div class="tree-body">
      <button class="node top" class:on={app.logFilter.branch === null} onclick={() => setBranch(null)}>All branches</button>
      <button class="node top" class:on={app.logFilter.branch === 'HEAD'} onclick={() => setBranch('HEAD')}>HEAD (Current Branch)</button>

      <button class="node head" onclick={() => (collapsed.local = !collapsed.local)}>{collapsed.local ? '▸' : '▾'} Local <span class="dim">{locals.length}</span></button>
      {#if !collapsed.local}
        {#each locals as b (b.full)}
          <button
            class="node leaf"
            class:on={app.logFilter.branch === b.name}
            class:current={b.current}
            onclick={() => setBranch(b.name)}
            ondblclick={() => !b.current && checkout(b)}
            oncontextmenu={(e) => treeMenu(b, e)}
            title={(b.upstream ? `tracks ${b.upstream}` : 'no upstream') + (otherWorktree(b) ? `\nchecked out in worktree ${b.worktree}` : '')}
          >
            <span class="ellipsis grow">{b.current ? '◆ ' : ''}{b.name}</span>
            {#if otherWorktree(b)}<span class="wt" title="Checked out in worktree {b.worktree}">⧉</span>{/if}
            {#if b.ahead}<span class="ab">↑{b.ahead}</span>{/if}
            {#if b.behind}<span class="ab">↓{b.behind}</span>{/if}
            {#if b.gone}<span class="dim small">gone</span>{/if}
          </button>
        {/each}
      {/if}

      <button class="node head" onclick={() => (collapsed.remote = !collapsed.remote)}>{collapsed.remote ? '▸' : '▾'} Remote</button>
      {#if !collapsed.remote}
        {#each Object.entries(remotesByName) as [rname, list] (rname)}
          <button class="node sub" onclick={() => (collapsed['r:' + rname] = !collapsed['r:' + rname])}>{collapsed['r:' + rname] ? '▸' : '▾'} {rname} <span class="dim">{list.length}</span></button>
          {#if !collapsed['r:' + rname]}
            {#each list as b (b.full)}
              <button class="node leaf deep" class:on={app.logFilter.branch === b.name} onclick={() => setBranch(b.name)} ondblclick={() => checkout(b)} oncontextmenu={(e) => treeMenu(b, e)}>
                <span class="ellipsis">{b.name.slice(rname.length + 1)}</span>
              </button>
            {/each}
          {/if}
        {/each}
      {/if}

      <button class="node head" onclick={() => (collapsed.tags = !collapsed.tags)}>{collapsed.tags ? '▸' : '▾'} Tags <span class="dim">{tags.length}</span></button>
      {#if !collapsed.tags}
        {#each tags as b (b.full)}
          <button class="node leaf" class:on={app.logFilter.branch === b.name} onclick={() => setBranch(b.name)} oncontextmenu={(e) => treeMenu(b, e)}>
            <span class="ellipsis">{b.name}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
  <div class="splitter" use:splitter={{ onStart: () => (s0 = tree), onDelta: (dx) => { tree = Math.max(140, Math.min(500, s0 + dx)); treeW.set(tree) } }}></div>

  <!-- commit table -->
  <div class="center" style={colStyle} bind:clientWidth={centerW}>
    <div class="filters row">
      <input type="text" class="grow" placeholder="Text or hash in message" bind:value={text} spellcheck="false" />
      <input type="text" style="width:130px" placeholder="User" bind:value={author} spellcheck="false" />
      <input type="text" style="width:170px" placeholder="Path" bind:value={app.logFilter.path} spellcheck="false" />
      {#if app.logFilter.branch}
        <span class="chip">⎇ {app.logFilter.branch} <button class="flat x" onclick={() => setBranch(null)}>×</button></span>
      {/if}
      {#if filtered}<button class="flat" onclick={() => { text = ''; author = ''; app.logFilter.path = '' }}>Clear</button>{/if}
    </div>
    <div class="thead row">
      <span class="th-graph" style="width:{layout.graph}px"></span>
      <span class="subject ellipsis">Subject</span>
      <span class="refs"></span>
      {#each COLS as col}
        <span class="c-{col} th">
          {{ author: 'Author', date: 'Date', hash: 'Hash' }[col]}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            class="col-grip"
            title="Drag to resize · double-click to fit content{colW[col] ? '' : ' (auto)'}"
            use:splitter={colDrag(col)}
            ondblclick={() => colAuto(col)}
          ></span>
        </span>
      {/each}
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={onScroll} tabindex="0" onkeydown={onKey} role="grid">
      {#if error}
        <div class="empty">{error}</div>
      {:else if !commits.length && !loading}
        <div class="empty">No commits</div>
      {/if}
      <div style="height:{commits.length * ROW}px;position:relative">
        {#each commits.slice(start, end) as c, i (c.hash)}
          {@const idx = start + i}
          {@const g = graph[idx]}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="crow row"
            class:sel={markedSet.has(c.hash)}
            class:merge={c.parents.length > 1}
            style="top:{idx * ROW}px"
            onclick={(e) => clickRow(c, idx, e)}
            oncontextmenu={(e) => commitMenu(c, e)}
          >
            <svg class="graph" width={layout.graph} height={ROW}>
              {#if g && !filtered}
                {#each g.passes as p}<path d={curve(x(p.from), 0, x(p.to), ROW)} stroke={laneColor(p.color)} />{/each}
                {#each g.incoming as p}<path d={curve(x(p.from), 0, x(g.col), ROW / 2)} stroke={laneColor(p.color)} />{/each}
                {#each g.outgoing as p}<path d={curve(x(g.col), ROW / 2, x(p.to), ROW)} stroke={laneColor(p.color)} />{/each}
                <circle cx={x(g.col)} cy={ROW / 2} r={c.parents.length > 1 ? 3 : 4} fill={laneColor(g.color)} stroke="var(--bg2)" stroke-width="1.5"><title>{c.inMain ? 'Already in main' : 'Not in main yet'}</title></circle>
              {:else}
                <circle cx="9" cy={ROW / 2} r="3.5" fill={c.inMain ? laneColor(MAIN) : 'var(--accent)'}><title>{c.inMain ? 'Already in main' : 'Not in main yet'}</title></circle>
              {/if}
            </svg>
            <span class="subject ellipsis" class:dim={c.parents.length > 1}>{c.subject}</span>
            <span class="refs">
              {#each c.refs as r}
                <span class="ref {r.kind}" class:current={r.current} title={r.kind}>{r.kind === 'tag' ? '🏷 ' : ''}{r.name}</span>
              {/each}
            </span>
            <span class="c-author ellipsis muted">{c.author}</span>
            <span class="c-date ellipsis muted">{fmtDate(c.time)}</span>
            <span class="c-hash mono dim">{short(c.hash)}</span>
          </div>
        {/each}
      </div>
      {#if loading && !commits.length}<div class="empty">Loading…</div>{/if}
    </div>
  </div>
  <div class="splitter" use:splitter={{ onStart: () => (s0 = details), onDelta: (dx) => { details = Math.max(240, Math.min(800, s0 - dx)); detW.set(details) } }}></div>

  <!-- details -->
  <div class="details" style="width:{details}px">
    {#if detail}
      <div class="files-head section-title row">
        <span class="grow">Changed files <span class="dim">{detail.files.length}</span></span>
        <button class="flat tiny" class:on={grouped} title={grouped ? 'Grouped by directory — click for a flat list' : 'Group by directory'} onclick={toggleGrouping}>🗀 Folders</button>
      </div>
      <div class="cfiles">
        {#each fileRows as row (row.key)}
          {#if row.kind === 'dir'}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="cfile row cdir" style="padding-left:{10 + row.depth * 16}px" title={row.dir} onclick={() => (collapsedDirs[row.key] = !collapsedDirs[row.key])}>
              <span class="caret dim">{collapsedDirs[row.key] ? '▸' : '▾'}</span>
              <span class="ellipsis">🗀 {row.name}</span>
              <span class="dim small">{row.count}</span>
            </div>
          {:else}
          {@const f = row.item}
          {@const sp = splitPath(f.path)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="cfile row" style="padding-left:{10 + row.depth * 16 + (grouped ? 14 : 0)}px" class:on={diffFile?.path === f.path} onclick={() => openFile(f)} title={(statusName[f.status] ?? f.status) + ': ' + (f.origPath ? `${f.origPath} → ` : '') + f.path}>
            <span class="st st-{f.status}">{f.status}</span>
            <span class="fname fname-{f.status} ellipsis">{sp.name}</span>
            <span class="dim ellipsis grow small">{grouped ? '' : sp.dir}</span>
          </div>
          {/if}
        {/each}
      </div>
      <div class="meta selectable">
        <div class="msg">{detail.message}</div>
        <div class="kv">
          <span class="dim">Commit</span>
          <span class="mono">{detail.hash.slice(0, 12)} <button class="flat tiny" onclick={() => copyText(detail!.hash)}>copy</button></span>
          <span class="dim">Author</span><span>{detail.author} &lt;{detail.email}&gt;</span>
          <span class="dim">Date</span><span>{fmtDate(detail.authorTime)}</span>
          {#if detail.committer !== detail.author || detail.commitTime !== detail.authorTime}
            <span class="dim">Committer</span><span>{detail.committer}, {fmtDate(detail.commitTime)}</span>
          {/if}
          {#if detail.parents.length}
            <span class="dim">Parents</span>
            <span>
              {#each detail.parents as p}<button class="flat tiny mono" onclick={() => select(p)}>{short(p)}</button>{/each}
            </span>
          {/if}
        </div>
      </div>
    {:else}
      <div class="empty">Select a commit</div>
    {/if}
  </div>
</div>

{#if diffFile}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="modal-back" onclick={(e) => e.target === e.currentTarget && (diffFile = null)}>
    <div class="modal">
      <div class="modal-head row">
        <span class="grow ellipsis"><b>{diffFile.path}</b> <span class="dim">· {short(detail?.hash ?? '')} {detail?.message.split('\n')[0]}</span></span>
        <button class="flat" onclick={() => stepFile(-1)} title="Previous file">◀</button>
        <button class="flat" onclick={() => stepFile(1)} title="Next file">▶</button>
        <button class="flat" onclick={() => (diffFile = null)}>✕</button>
      </div>
      <div class="modal-body"><DiffView diff={fileDiff} mode="readonly" title={diffFile.origPath ? `${diffFile.origPath} → ${diffFile.path}` : diffFile.path} /></div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => diffFile && e.key === 'Escape' && (diffFile = null)} />

<style>
  .wt {
    color: var(--accent);
    font-size: 11px;
  }
  .logview {
    display: flex;
    height: 100%;
    min-height: 0;
  }
  .tree {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    min-width: 0;
  }
  .tree-search {
    padding: 6px;
  }
  .tree-search input {
    width: 100%;
  }
  .tree-body {
    flex: 1;
    overflow-y: auto;
    padding: 0 4px 8px;
  }
  .node {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-radius: 4px;
    padding: 2px 6px;
    height: 22px;
  }
  .node:hover {
    background: var(--hover);
  }
  .node.on {
    background: var(--sel);
  }
  .node.head {
    font-weight: 600;
    margin-top: 6px;
    font-size: 12px;
  }
  .node.top {
    color: var(--fg2);
  }
  .node.sub {
    padding-left: 16px;
    font-size: 12.5px;
  }
  .node.leaf {
    padding-left: 22px;
    font-size: 12.5px;
  }
  .node.deep {
    padding-left: 34px;
  }
  .node.current {
    color: var(--warn);
    font-weight: 600;
  }
  .ab {
    font-size: 11px;
    color: var(--accent);
  }
  .small {
    font-size: 11px;
  }
  .center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg2);
  }
  .filters {
    padding: 6px;
    gap: 6px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    background: var(--sel);
    border-radius: 10px;
    padding: 0 0 0 8px;
    font-size: 12px;
    white-space: nowrap;
  }
  .chip .x {
    padding: 0 6px;
  }
  .thead {
    font-size: 11.5px;
    color: var(--fg3);
    padding: 3px 8px 3px 0;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  .scroller {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
    position: relative;
  }
  .crow {
    position: absolute;
    left: 0;
    right: 0;
    height: 24px;
    padding-right: 8px;
    gap: 6px;
    white-space: nowrap;
  }
  .crow:hover {
    background: var(--hover);
  }
  .crow.sel {
    background: var(--sel-inactive);
  }
  .scroller:focus .crow.sel {
    background: var(--sel);
  }
  .graph,
  .th-graph {
    flex: none;
  }
  .graph path {
    fill: none;
    stroke-width: 1.6;
  }
  .subject {
    flex: 1 1 0;
    min-width: 90px;
  }
  .refs {
    display: flex;
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    mask-image: linear-gradient(to right, #000 85%, transparent);
  }
  .refs .ref {
    flex: none;
  }
  .c-author {
    width: var(--w-author, 130px);
    flex: none;
  }
  .c-date {
    width: var(--w-date, 140px);
    flex: none;
  }
  .c-hash {
    width: var(--w-hash, 70px);
    flex: none;
  }
  .th {
    position: relative;
    overflow: visible;
    white-space: nowrap;
  }
  .col-grip {
    position: absolute;
    top: -3px;
    bottom: -3px;
    left: -7px;
    width: 8px;
    cursor: col-resize;
  }
  .col-grip::after {
    content: '';
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 3px;
    width: 1px;
    background: var(--border);
  }
  .col-grip:hover::after {
    background: var(--accent);
    width: 2px;
  }
  .details {
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    min-width: 0;
  }
  .cfiles {
    flex: 1;
    overflow-y: auto;
    min-height: 80px;
  }
  .cfile {
    padding: 2px 10px;
    gap: 6px;
    height: 22px;
    cursor: pointer;
  }
  .cfile:hover {
    background: var(--hover);
  }
  .cfile.on {
    background: var(--sel);
  }
  .cdir {
    gap: 4px;
  }
  .caret {
    width: 10px;
  }
  .files-head button {
    text-transform: none;
    letter-spacing: normal;
    font-weight: normal;
  }
  .files-head .on {
    background: var(--sel);
  }
  .fname {
    flex: none;
    max-width: 65%;
  }
  .meta {
    border-top: 1px solid var(--border);
    padding: 10px 12px;
    max-height: 50%;
    overflow-y: auto;
    flex: none;
  }
  .msg {
    white-space: pre-wrap;
    margin-bottom: 10px;
    word-break: break-word;
  }
  .kv {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 10px;
    font-size: 12px;
  }
  .tiny {
    padding: 0 4px;
    font-size: 11px;
  }
  .modal-back {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 60;
    display: flex;
    padding: 30px;
  }
  .modal {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border2);
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
  }
  .modal-head {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .modal-body {
    flex: 1;
    min-height: 0;
  }
</style>
