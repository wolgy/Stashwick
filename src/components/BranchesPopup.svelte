<script lang="ts">
  import { app, showMenu, fmtDate } from '../lib/store.svelte'
  import type { BranchInfo } from '../lib/api'
  import { branchMenuItems, newBranch, checkoutRevision, checkout, newWorktree, otherWorktree } from '../lib/actions'
  import { tick } from 'svelte'

  let q = $state('')
  let input = $state<HTMLInputElement>()
  let active = $state(0)

  $effect(() => {
    if (app.branchesOpen) {
      q = ''
      active = 0
      tick().then(() => input?.focus())
    }
  })

  const match = (b: BranchInfo) => {
    if (!q) return true
    // fuzzy-ish: all chars in order
    const s = b.name.toLowerCase()
    let i = 0
    for (const ch of q.toLowerCase()) {
      i = s.indexOf(ch, i)
      if (i === -1) return false
      i++
    }
    return true
  }
  const locals = $derived(
    app.branches
      .filter((b) => b.kind === 'local' && match(b))
      .sort((a, b) => Number(b.current) - Number(a.current) || b.time - a.time),
  )
  const remotes = $derived(app.branches.filter((b) => b.kind === 'remote' && match(b)).slice(0, q ? 200 : 40))
  const all = $derived([...locals, ...remotes])

  function close() {
    app.branchesOpen = false
  }
  function openMenu(b: BranchInfo, e: MouseEvent) {
    close()
    showMenu(e, branchMenuItems(b))
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close()
    else if (e.key === 'ArrowDown') {
      active = Math.min(all.length - 1, active + 1)
      e.preventDefault()
    } else if (e.key === 'ArrowUp') {
      active = Math.max(0, active - 1)
      e.preventDefault()
    } else if (e.key === 'Enter' && all[active]) {
      const b = all[active]
      close()
      if (!b.current) checkout(b)
    }
  }
  $effect(() => {
    q
    active = 0
  })
</script>

{#if app.branchesOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="back" onclick={close}></div>
  <div class="popup">
    <input bind:this={input} type="text" placeholder="Search branches  (Enter = checkout)" bind:value={q} onkeydown={onKey} spellcheck="false" />
    <div class="list">
      {#if !q}
        <button class="item act" onclick={() => { close(); newBranch() }}>＋ New Branch…</button>
        <button class="item act" onclick={() => { close(); checkoutRevision() }}>⎋ Checkout Tag or Revision…</button>
        <button class="item act" onclick={() => { close(); newWorktree() }}>⧉ New Worktree…</button>
        <button class="item act" onclick={() => { close(); app.worktreesOpen = true }}>⧉ Manage Worktrees…</button>
      {/if}
      {#if locals.length}<div class="section-title">Local</div>{/if}
      {#each locals as b, i (b.full)}
        <button class="item" class:active={active === i} onclick={(e) => openMenu(b, e)} onmouseenter={() => (active = i)}>
          <span class="star">{b.current ? '◆' : ''}</span>
          <span class="name ellipsis grow" class:cur={b.current}>{b.name}</span>
          {#if otherWorktree(b)}<span class="ab" title="Checked out in worktree {b.worktree}">⧉ worktree</span>{/if}
          {#if b.ahead}<span class="ab">↑{b.ahead}</span>{/if}
          {#if b.behind}<span class="ab">↓{b.behind}</span>{/if}
          {#if b.upstream}<span class="dim up ellipsis">{b.gone ? 'upstream gone' : b.upstream}</span>{/if}
          <span class="chev">›</span>
        </button>
      {/each}
      {#if remotes.length}<div class="section-title">Remote</div>{/if}
      {#each remotes as b, i (b.full)}
        <button class="item" class:active={active === locals.length + i} onclick={(e) => openMenu(b, e)} onmouseenter={() => (active = locals.length + i)} title={fmtDate(b.time)}>
          <span class="star"></span>
          <span class="name ellipsis grow">{b.name}</span>
          <span class="chev">›</span>
        </button>
      {/each}
      {#if !all.length}<div class="dim pad">No branches match “{q}”</div>{/if}
    </div>
  </div>
{/if}

<style>
  .back {
    position: fixed;
    inset: 0;
    z-index: 70;
  }
  .popup {
    position: fixed;
    top: 44px;
    left: 50%;
    transform: translateX(-50%);
    width: 480px;
    max-height: 70vh;
    z-index: 71;
    background: var(--raised);
    border: 1px solid var(--border2);
    border-radius: 8px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    padding: 8px;
    gap: 6px;
  }
  input {
    width: 100%;
  }
  .list {
    overflow-y: auto;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-radius: 4px;
    padding: 3px 8px;
  }
  .item.active,
  .item:hover {
    background: var(--accent);
    color: #fff;
  }
  .item.active .dim,
  .item:hover .dim {
    color: #ffffffbb;
  }
  .act {
    color: var(--fg2);
  }
  .star {
    width: 12px;
    color: var(--warn);
    font-size: 10px;
  }
  .cur {
    font-weight: 600;
  }
  .ab {
    font-size: 11px;
  }
  .up {
    font-size: 11px;
    max-width: 150px;
  }
  .chev {
    color: var(--fg3);
  }
  .pad {
    padding: 10px;
  }
</style>
