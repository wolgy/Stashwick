<script lang="ts">
  import { api, type ConflictVersions } from '../lib/api'
  import { app, repo, op, confirm, toast, openInEditor } from '../lib/store.svelte'
  import { merge3, buildResult, resolvedLines, type Chunk, type Resolution } from '../lib/diff3'

  let v = $state<ConflictVersions | null>(null)
  let chunks = $state<Chunk[]>([])
  let error = $state<string | null>(null)
  let editing = $state<number | null>(null)
  let expanded = $state<Record<number, boolean>>({})

  $effect(() => {
    const f = app.mergeFile
    const r = repo()
    v = null
    chunks = []
    error = null
    editing = null
    expanded = {}
    if (!f || !r) return
    api
      .conflictVersions(r, f)
      .then((x) => {
        v = x
        if (x.ours !== null && x.theirs !== null && !x.binary) chunks = merge3(x.base ?? '', x.ours, x.theirs)
      })
      .catch((e) => (error = String(e)))
  })

  const conflicts = $derived(chunks.filter((c) => c.kind === 'conflict') as Extract<Chunk, { kind: 'conflict' }>[])
  const unresolved = $derived(conflicts.filter((c) => c.resolution === null).length)
  const autoCount = $derived(chunks.filter((c) => c.kind === 'auto').length)
  const deletedSide = $derived(v && !v.binary && (v.ours === null || v.theirs === null))

  function close() {
    app.mergeFile = null
  }

  function resolve(i: number, r: Resolution) {
    const c = chunks[i]
    if (c.kind !== 'conflict') return
    if (r === 'custom' && c.resolution !== 'custom') c.custom = resolvedLines(c).join('')
    c.resolution = r
    editing = r === 'custom' ? i : null
  }

  async function resolveAll(side: 'ours' | 'theirs') {
    for (const c of conflicts) c.resolution = side
  }

  async function save() {
    const f = app.mergeFile
    if (!f) return
    if (unresolved && !(await confirm('Unresolved Conflicts', `${unresolved} conflict(s) are not resolved. Save with conflict markers and mark the file as resolved anyway?`, 'Save Anyway', true))) return
    const content = buildResult(chunks, v?.oursLabel, v?.theirsLabel)
    const res = await op(`Resolve ${f}`, (r) => api.resolveConflict(r, f, content))
    if (res !== undefined) {
      toast('success', `${f} resolved`)
      nextOrClose()
    }
  }

  async function acceptSide(side: 'ours' | 'theirs') {
    const f = app.mergeFile
    if (!f) return
    const res = await op(`Accept ${side} for ${f}`, (r) => api.acceptSide(r, f, side))
    if (res !== undefined) nextOrClose()
  }

  function nextOrClose() {
    const next = app.status?.files.find((x) => x.conflict && x.path !== app.mergeFile)
    app.mergeFile = next ? next.path : null
  }

  const lines = (arr: string[]) => arr.map((l) => l.replace(/\r?\n$/, ''))
  const COLLAPSE = 8
</script>

{#if app.mergeFile}
  <div class="back">
    <div class="tool">
      <div class="head row">
        <div class="grow">
          <div class="title">Merge Conflict: <span class="mono">{app.mergeFile}</span></div>
          {#if app.status?.state.detail}<div class="dim small">{app.status.state.detail}</div>{/if}
        </div>
        {#if chunks.length}
          <span class="muted small">{conflicts.length} conflict{conflicts.length === 1 ? '' : 's'}, {unresolved} unresolved · {autoCount} change{autoCount === 1 ? '' : 's'} merged automatically</span>
        {/if}
        <button class="flat" onclick={() => openInEditor([app.mergeFile!])} title="Open the file (with conflict markers) in your editor">Open in Editor</button>
        <button class="flat" onclick={close}>✕</button>
      </div>

      {#if error}
        <div class="empty">{error}</div>
      {:else if !v}
        <div class="empty">Loading…</div>
      {:else if v.binary || deletedSide}
        <div class="empty">
          <div>
            {#if v.binary}
              <p>Binary file — choose which version to keep.</p>
            {:else}
              <p>
                {v.ours === null ? `Deleted in ${v.oursLabel}, modified in ${v.theirsLabel}.` : `Modified in ${v.oursLabel}, deleted in ${v.theirsLabel}.`}
              </p>
            {/if}
            <div class="row" style="justify-content:center;gap:10px">
              <button onclick={() => acceptSide('ours')}>{v.ours === null ? 'Delete file' : 'Keep'} — {v.oursLabel}</button>
              <button onclick={() => acceptSide('theirs')}>{v.theirs === null ? 'Delete file' : 'Keep'} — {v.theirsLabel}</button>
            </div>
          </div>
        </div>
      {:else}
        <div class="cols row">
          <div class="col-h ours">{v.oursLabel}</div>
          <div class="col-h result">Result</div>
          <div class="col-h theirs">{v.theirsLabel}</div>
        </div>
        <div class="grid mono">
          {#each chunks as c, i}
            {#if c.kind === 'same'}
              {@const ls = lines(c.lines)}
              {#if ls.length > COLLAPSE * 2 + 3 && !expanded[i]}
                <div class="chunk same">
                  <pre>{ls.slice(0, COLLAPSE).join('\n')}</pre><pre>{ls.slice(0, COLLAPSE).join('\n')}</pre><pre>{ls.slice(0, COLLAPSE).join('\n')}</pre>
                </div>
                <button class="fold" onclick={() => (expanded[i] = true)}>⋯ {ls.length - COLLAPSE * 2} unchanged lines ⋯</button>
                <div class="chunk same">
                  <pre>{ls.slice(-COLLAPSE).join('\n')}</pre><pre>{ls.slice(-COLLAPSE).join('\n')}</pre><pre>{ls.slice(-COLLAPSE).join('\n')}</pre>
                </div>
              {:else}
                <div class="chunk same"><pre>{ls.join('\n')}</pre><pre>{ls.join('\n')}</pre><pre>{ls.join('\n')}</pre></div>
              {/if}
            {:else if c.kind === 'auto'}
              <div class="chunk auto">
                <pre class:changed={c.side !== 'theirs'}>{lines(c.side === 'theirs' ? c.base : c.lines).join('\n')}</pre>
                <pre class="res">{lines(c.lines).join('\n')}</pre>
                <pre class:changed={c.side !== 'ours'}>{lines(c.side === 'ours' ? c.base : c.lines).join('\n')}</pre>
              </div>
            {:else}
              <div class="chunk conflict" class:resolved={c.resolution !== null}>
                <div class="side">
                  <pre>{lines(c.ours).join('\n')}</pre>
                  <button class="take" onclick={() => resolve(i, 'ours')} title="Accept this side">Accept »</button>
                </div>
                <div class="mid">
                  {#if editing === i}
                    <textarea class="mono" rows={Math.max(3, c.custom.split('\n').length)} bind:value={c.custom}></textarea>
                  {:else if c.resolution === null}
                    <pre class="unres">{lines(resolvedLines(c, v.oursLabel, v.theirsLabel)).join('\n')}</pre>
                  {:else}
                    <pre class="res">{lines(resolvedLines(c)).join('\n') || ' '}</pre>
                  {/if}
                  <div class="opts row">
                    <button class="flat mini" class:on={c.resolution === 'ours'} onclick={() => resolve(i, 'ours')}>Left</button>
                    <button class="flat mini" class:on={c.resolution === 'theirs'} onclick={() => resolve(i, 'theirs')}>Right</button>
                    <button class="flat mini" class:on={c.resolution === 'ours+theirs'} onclick={() => resolve(i, 'ours+theirs')}>Left+Right</button>
                    <button class="flat mini" class:on={c.resolution === 'theirs+ours'} onclick={() => resolve(i, 'theirs+ours')}>Right+Left</button>
                    <button class="flat mini" class:on={c.resolution === 'base'} onclick={() => resolve(i, 'base')} title="Keep the common ancestor's version">Base</button>
                    <button class="flat mini" class:on={c.resolution === 'custom'} onclick={() => resolve(i, 'custom')}>Edit</button>
                    {#if c.resolution !== null}<button class="flat mini" onclick={() => resolve(i, null)}>Reset</button>{/if}
                  </div>
                </div>
                <div class="side">
                  <pre>{lines(c.theirs).join('\n')}</pre>
                  <button class="take" onclick={() => resolve(i, 'theirs')} title="Accept this side">« Accept</button>
                </div>
              </div>
            {/if}
          {/each}
        </div>
        <div class="foot row">
          <button onclick={() => acceptSide('ours')}>Accept Left (whole file)</button>
          <button onclick={() => acceptSide('theirs')}>Accept Right (whole file)</button>
          {#if conflicts.length > 1}
            <button class="flat" onclick={() => resolveAll('ours')}>All conflicts → Left</button>
            <button class="flat" onclick={() => resolveAll('theirs')}>All conflicts → Right</button>
          {/if}
          <span class="grow"></span>
          <button onclick={close}>Cancel</button>
          <button class="primary" onclick={save}>Save & Mark Resolved</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .back {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 65;
    display: flex;
    padding: 20px;
  }
  .tool {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border2);
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  .head {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    gap: 10px;
  }
  .title {
    font-weight: 600;
  }
  .small {
    font-size: 12px;
  }
  .cols {
    display: grid !important;
    grid-template-columns: 1fr 1fr 1fr;
    border-bottom: 1px solid var(--border);
  }
  .col-h {
    padding: 4px 10px;
    font-size: 12px;
    font-weight: 600;
  }
  .col-h.ours {
    color: #6e9fff;
  }
  .col-h.theirs {
    color: #73bd79;
  }
  .col-h.result {
    text-align: center;
    border-left: 1px solid var(--border);
    border-right: 1px solid var(--border);
  }
  .grid {
    flex: 1;
    overflow: auto;
    background: var(--bg2);
    font-size: 12px;
    line-height: 18px;
  }
  .chunk {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
  }
  .chunk > *,
  .chunk pre {
    min-width: 0;
  }
  pre {
    margin: 0;
    padding: 0 10px;
    white-space: pre-wrap;
    word-break: break-all;
    tab-size: 4;
    user-select: text;
    -webkit-user-select: text;
  }
  .chunk > :nth-child(2) {
    border-left: 1px solid var(--border);
    border-right: 1px solid var(--border);
  }
  .same pre {
    color: var(--fg2);
  }
  .auto pre.changed,
  .auto pre.res {
    background: var(--add-bg);
  }
  .conflict {
    border-top: 1px solid var(--warn);
    border-bottom: 1px solid var(--warn);
    margin: 2px 0;
  }
  .conflict .side {
    background: var(--conf-bg);
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .conflict .side pre {
    flex: 1;
    padding-top: 2px;
    padding-bottom: 2px;
  }
  .conflict.resolved .side {
    background: transparent;
    opacity: 0.75;
  }
  .take {
    align-self: flex-end;
    margin: 2px 6px 4px;
    font-family: var(--ui);
    font-size: 11px;
    padding: 0 8px;
  }
  .side:last-child .take {
    align-self: flex-start;
  }
  .mid {
    display: flex;
    flex-direction: column;
  }
  .mid pre {
    flex: 1;
    padding-top: 2px;
  }
  .unres {
    background: var(--conf-bg);
    color: var(--fg2);
  }
  .res {
    background: var(--add-bg);
  }
  .mid textarea {
    margin: 4px;
    font-size: 12px;
    line-height: 18px;
    resize: vertical;
    font-family: var(--mono);
  }
  .opts {
    flex-wrap: wrap;
    gap: 2px;
    padding: 2px 4px 4px;
    font-family: var(--ui);
  }
  .mini {
    font-size: 11px;
    padding: 0 6px;
  }
  .mini.on {
    background: var(--sel);
  }
  .fold {
    width: 100%;
    border: none;
    border-radius: 0;
    background: var(--panel);
    color: var(--fg3);
    font-size: 11px;
    padding: 1px;
  }
  .foot {
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    gap: 8px;
  }
</style>
