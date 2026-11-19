<script lang="ts">
  import type { FileDiff, DiffLine } from '../lib/api'

  type Mode = 'unstaged' | 'staged' | 'readonly'
  type Action = 'stage' | 'unstage' | 'discard'
  let {
    diff,
    mode = 'readonly',
    title = '',
    onApply,
  }: {
    diff: FileDiff | null
    mode?: Mode
    title?: string
    onApply?: (action: Action, sel: { hunk: number; line: number }[]) => void
  } = $props()

  let sideBySide = $state(localStorage.getItem('stashwick.sbs') !== '0')
  let selected = $state(new Set<string>())
  let lastClicked: string | null = null

  // reset the selection only when the file or its content really changes
  const identity = $derived(`${title}\u0000${diff?.fingerprint ?? ''}`)
  $effect(() => {
    identity
    selected = new Set()
    lastClicked = null
    showAll = false
  })

  // Rendering is the bottleneck for huge diffs (one <tr> per line), so cap the
  // rows shown until asked. caps[hi] = how many rows of hunk hi to render.
  const RENDER_LIMIT = 3000
  let showAll = $state(false)
  const caps = $derived.by(() => {
    let budget = showAll ? Infinity : RENDER_LIMIT
    return (diff?.hunks ?? []).map((h) => {
      const n = Math.min(h.lines.length, budget)
      budget -= n
      return n
    })
  })
  const hiddenLines = $derived((diff?.hunks ?? []).reduce((acc, h, hi) => acc + h.lines.length - (caps[hi] ?? 0), 0))

  const key = (h: number, l: number) => `${h}:${l}`
  const isChange = (l: DiffLine) => l.kind === '+' || l.kind === '-'
  const changeKeys = $derived(
    (diff?.hunks ?? []).flatMap((h, hi) => h.lines.flatMap((l, li) => (isChange(l) ? [key(hi, li)] : []))),
  )
  const selectable = $derived(mode !== 'readonly' && !!onApply && !diff?.untracked)
  const stats = $derived.by(() => {
    let a = 0
    let d = 0
    for (const h of diff?.hunks ?? []) for (const l of h.lines) l.kind === '+' ? a++ : l.kind === '-' ? d++ : 0
    return { a, d }
  })

  function toggle(h: number, l: number, e: MouseEvent) {
    if (!selectable) return
    const k = key(h, l)
    const next = new Set(selected)
    if (e.shiftKey && lastClicked) {
      const a = changeKeys.indexOf(lastClicked)
      const b = changeKeys.indexOf(k)
      const [s, t] = a < b ? [a, b] : [b, a]
      const on = !selected.has(k)
      for (const kk of changeKeys.slice(s, t + 1)) on ? next.add(kk) : next.delete(kk)
    } else {
      next.has(k) ? next.delete(k) : next.add(k)
    }
    lastClicked = k
    selected = next
  }

  function selectHunk(hi: number) {
    return diff!.hunks[hi].lines.flatMap((l, li) => (isChange(l) ? [{ hunk: hi, line: li }] : []))
  }
  function selection() {
    return [...selected].map((k) => {
      const [hunk, line] = k.split(':').map(Number)
      return { hunk, line }
    })
  }
  function setSbs(v: boolean) {
    sideBySide = v
    localStorage.setItem('stashwick.sbs', v ? '1' : '0')
  }

  type Cell = { hi: number; li: number; line: DiffLine } | null
  function pairRows(hi: number) {
    const rows: { l: Cell; r: Cell }[] = []
    const lines = diff!.hunks[hi].lines
    let i = 0
    while (i < lines.length) {
      const ln = lines[i]
      if (ln.kind === ' ' || ln.kind === '\\') {
        rows.push({ l: { hi, li: i, line: ln }, r: { hi, li: i, line: ln } })
        i++
        continue
      }
      const dels: Cell[] = []
      const adds: Cell[] = []
      while (i < lines.length && lines[i].kind === '-') dels.push({ hi, li: i, line: lines[i++] })
      while (i < lines.length && lines[i].kind === '\\') i++
      while (i < lines.length && lines[i].kind === '+') adds.push({ hi, li: i, line: lines[i++] })
      while (i < lines.length && lines[i].kind === '\\') i++
      for (let k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ l: dels[k] ?? null, r: adds[k] ?? null })
    }
    return rows
  }
</script>

<div class="diffview">
  <div class="bar">
    <span class="title ellipsis grow mono">{title}</span>
    {#if diff && !diff.binary && !diff.tooLarge}
      <span class="add">+{stats.a}</span><span class="del">−{stats.d}</span>
    {/if}
    {#if selectable && selected.size}
      <span class="muted">{selected.size} line{selected.size > 1 ? 's' : ''} selected</span>
      {#if mode === 'unstaged'}
        <button class="primary" onclick={() => onApply?.('stage', selection())}>Stage Selected</button>
        <button onclick={() => onApply?.('discard', selection())}>Rollback Selected</button>
      {:else}
        <button class="primary" onclick={() => onApply?.('unstage', selection())}>Unstage Selected</button>
      {/if}
      <button class="flat" onclick={() => (selected = new Set())}>Clear</button>
    {/if}
    <div class="seg">
      <button class:on={!sideBySide} onclick={() => setSbs(false)} title="Unified">Unified</button>
      <button class:on={sideBySide} onclick={() => setSbs(true)} title="Side-by-side">Side-by-side</button>
    </div>
  </div>

  <div class="body mono">
    {#if !diff}
      <div class="empty">Select a file to see changes</div>
    {:else if diff.note}
      <div class="empty">{diff.note}</div>
    {:else if diff.binary}
      <div class="empty">Binary file</div>
    {:else if diff.tooLarge}
      <div class="empty">File too large to display</div>
    {:else if !diff.hunks.length}
      <div class="empty">{diff.header.some((h) => h.startsWith('old mode')) ? 'File mode changed' : diff.header.some((h) => h.startsWith('rename from')) ? 'Renamed without content changes' : 'No content changes'}</div>
    {:else}
      {#if diff.untracked && mode === 'unstaged'}
        <div class="info">New untracked file — stage it as a whole from the file list.</div>
      {/if}
      {#each diff.hunks as h, hi}
        {#if caps[hi]}
        <div class="hunk-head">
          <span class="grow ellipsis">{h.header}</span>
          {#if selectable}
            {#if mode === 'unstaged'}
              <button class="flat small" onclick={() => onApply?.('stage', selectHunk(hi))}>Stage Hunk</button>
              <button class="flat small" onclick={() => onApply?.('discard', selectHunk(hi))}>Rollback Hunk</button>
            {:else}
              <button class="flat small" onclick={() => onApply?.('unstage', selectHunk(hi))}>Unstage Hunk</button>
            {/if}
          {/if}
        </div>
        {#if sideBySide}
          <table class="sbs">
            <tbody>
              {#each pairRows(hi).slice(0, caps[hi]) as row}
                <tr>
                  {#each [row.l, row.r] as c, side}
                    {#if c}
                      {@const k = key(c.hi, c.li)}
                      {@const ch = isChange(c.line)}
                      <td
                        class="gut"
                        class:sel={selected.has(k)}
                        class:clickable={selectable && ch}
                        onclick={(e) => ch && toggle(c.hi, c.li, e)}
                      >{side === 0 ? (c.line.oldNo ?? '') : (c.line.newNo ?? '')}</td>
                      <td class="code {ch ? (c.line.kind === '+' ? 'add-l' : 'del-l') : ''}" class:selrow={selected.has(k)}>{c.line.text}</td>
                    {:else}
                      <td class="gut empty-cell"></td><td class="code empty-cell"></td>
                    {/if}
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <table class="uni">
            <tbody>
              {#each h.lines.slice(0, caps[hi]) as l, li}
                {@const k = key(hi, li)}
                {@const ch = isChange(l)}
                <tr class={l.kind === '+' ? 'add-l' : l.kind === '-' ? 'del-l' : l.kind === '\\' ? 'noeol' : ''} class:selrow={selected.has(k)}>
                  <td class="gut" class:sel={selected.has(k)} class:clickable={selectable && ch} onclick={(e) => ch && toggle(hi, li, e)}>{l.oldNo ?? ''}</td>
                  <td class="gut" class:sel={selected.has(k)} class:clickable={selectable && ch} onclick={(e) => ch && toggle(hi, li, e)}>{l.newNo ?? ''}</td>
                  <td class="sign">{l.kind === '\\' ? '' : l.kind}</td>
                  <td class="code">{l.text}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
        {/if}
      {/each}
      {#if hiddenLines > 0}
        <div class="info">
          {hiddenLines.toLocaleString()} more lines not shown.
          <button class="flat small" onclick={() => (showAll = true)}>Show all</button>
        </div>
      {/if}
      {#if selectable}
        <div class="tip dim">Tip: click line numbers to select lines (Shift-click for a range), then stage or roll back just those lines.</div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .diffview {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--bg2);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    min-height: 34px;
  }
  .title {
    font-size: 12px;
  }
  .add {
    color: var(--ok);
    font-family: var(--mono);
    font-size: 12px;
  }
  .del {
    color: var(--danger);
    font-family: var(--mono);
    font-size: 12px;
  }
  .seg {
    display: flex;
  }
  .seg button {
    border-radius: 0;
    padding: 2px 8px;
    font-size: 12px;
  }
  .seg button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .seg button:last-child {
    border-radius: 0 4px 4px 0;
    border-left: none;
  }
  .seg button.on {
    background: var(--sel);
  }
  .body {
    flex: 1;
    overflow: auto;
    user-select: text;
    -webkit-user-select: text;
  }
  .info {
    padding: 6px 10px;
    color: var(--fg2);
    font-family: var(--ui);
  }
  .hunk-head {
    display: flex;
    align-items: center;
    gap: 4px;
    position: sticky;
    left: 0;
    background: var(--mod-bg);
    color: var(--fg2);
    padding: 2px 8px;
    font-size: 11.5px;
    border-top: 1px solid var(--border);
    user-select: none;
  }
  .small {
    font-family: var(--ui);
    font-size: 11.5px;
    padding: 0 6px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: 12px;
    line-height: 18px;
  }
  table.sbs {
    table-layout: fixed;
  }
  .sbs .gut {
    width: 52px;
  }
  .sbs td.code {
    width: calc(50% - 52px);
    overflow: hidden;
    text-overflow: clip;
  }
  .sbs td:nth-child(3) {
    border-left: 1px solid var(--border2);
  }
  td {
    padding: 0;
    vertical-align: top;
  }
  .gut {
    width: 46px;
    min-width: 46px;
    text-align: right;
    padding: 0 6px;
    color: var(--fg3);
    user-select: none;
    background: var(--panel);
    position: relative;
  }
  .gut.clickable {
    cursor: pointer;
  }
  .gut.clickable:hover {
    color: var(--fg);
    background: var(--hover);
  }
  .gut.sel,
  .gut.sel.clickable:hover {
    background: var(--accent);
    color: #fff;
  }
  .sign {
    width: 16px;
    text-align: center;
    color: var(--fg3);
    user-select: none;
  }
  .code {
    white-space: pre;
    padding-right: 12px;
    tab-size: 4;
  }
  .sbs .code {
    white-space: pre-wrap;
    word-break: break-all;
  }
  tr.add-l,
  td.add-l {
    background: var(--add-bg);
  }
  tr.del-l,
  td.del-l {
    background: var(--del-bg);
  }
  tr.add-l.selrow,
  td.add-l.selrow {
    background: var(--add-strong);
  }
  tr.del-l.selrow,
  td.del-l.selrow {
    background: var(--del-strong);
  }
  tr.noeol {
    color: var(--fg3);
    font-style: italic;
  }
  .empty-cell {
    background: repeating-linear-gradient(135deg, transparent 0 4px, var(--border) 4px 5px);
  }
  .tip {
    font-family: var(--ui);
    font-size: 11.5px;
    padding: 10px;
    user-select: none;
  }
</style>
