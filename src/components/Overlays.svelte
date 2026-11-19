<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { tick } from 'svelte'

  let values = $state<Record<string, any>>({})
  let dlgEl = $state<HTMLDivElement>()
  let menuEl = $state<HTMLDivElement>()
  let menuPos = $state({ x: 0, y: 0 })

  $effect(() => {
    const d = app.dialog
    if (!d) return
    const v: Record<string, any> = {}
    for (const f of d.fields ?? []) v[f.key] = f.value ?? (f.type === 'checkbox' ? false : f.type === 'select' ? f.options[0]?.value : '')
    values = v
    tick().then(() => {
      const el = dlgEl?.querySelector<HTMLElement>('input[type=text],input:not([type]),textarea,select') ?? dlgEl?.querySelector<HTMLElement>('button.primary, button.danger')
      el?.focus()
      if (el instanceof HTMLInputElement) el.select()
    })
  })

  $effect(() => {
    const m = app.menu
    if (!m) return
    menuPos = { x: m.x, y: m.y }
    tick().then(() => {
      if (!menuEl) return
      const r = menuEl.getBoundingClientRect()
      menuPos = {
        x: Math.min(m.x, window.innerWidth - r.width - 4),
        y: m.y + r.height > window.innerHeight ? Math.max(4, m.y - r.height) : m.y,
      }
    })
  })

  function close(ok: boolean) {
    const d = app.dialog
    if (!d) return
    app.dialog = null
    d.resolve(ok ? $state.snapshot(values) : null)
  }
  function onKey(e: KeyboardEvent) {
    if (app.menu && e.key === 'Escape') {
      app.menu = null
      return
    }
    if (!app.dialog) return
    if (e.key === 'Escape') close(false)
    if (e.key === 'Enter' && !(e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault()
      close(true)
    }
  }
</script>

<svelte:window onkeydown={onKey} onblur={() => (app.menu = null)} />

{#if app.menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="menu-backdrop" onclick={() => (app.menu = null)} oncontextmenu={(e) => { e.preventDefault(); app.menu = null }}></div>
  <div class="menu" bind:this={menuEl} style="left:{menuPos.x}px;top:{menuPos.y}px">
    {#each app.menu.items as it}
      {#if it === '-'}
        <div class="sep"></div>
      {:else}
        <button
          class="item"
          class:danger={it.danger}
          disabled={it.disabled}
          onclick={() => {
            app.menu = null
            it.action?.()
          }}
        >
          <span class="grow">{it.label}</span>
          {#if it.shortcut}<kbd>{it.shortcut}</kbd>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

{#if app.dialog}
  {@const d = app.dialog}
  <div class="backdrop">
    <div class="dialog" bind:this={dlgEl} role="dialog" aria-modal="true">
      <div class="title">{d.title}</div>
      {#if d.message}<div class="msg">{d.message}</div>{/if}
      {#if d.detail}<pre class="detail mono selectable">{d.detail}</pre>{/if}
      {#each d.fields ?? [] as f (f.key)}
        <label class="field" class:check={f.type === 'checkbox'}>
          {#if f.type === 'checkbox'}
            <input type="checkbox" bind:checked={values[f.key]} />
            <span>{f.label}</span>
          {:else}
            <span class="lbl">{f.label}</span>
            {#if f.type === 'textarea'}
              <textarea rows="5" bind:value={values[f.key]} placeholder={f.placeholder}></textarea>
            {:else if f.type === 'select'}
              <select bind:value={values[f.key]}>
                {#each f.options as o}<option value={o.value}>{o.label}</option>{/each}
              </select>
            {:else}
              <input type="text" bind:value={values[f.key]} placeholder={f.placeholder} spellcheck="false" />
            {/if}
          {/if}
        </label>
      {/each}
      <div class="buttons">
        {#if d.cancel !== null}<button onclick={() => close(false)}>{d.cancel ?? 'Cancel'}</button>{/if}
        <button class={d.danger ? 'danger' : 'primary'} onclick={() => close(true)}>{d.ok ?? 'OK'}</button>
      </div>
    </div>
  </div>
{/if}

<div class="toasts">
  {#each app.toasts as t (t.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="toast {t.kind}" onclick={() => (app.toasts = app.toasts.filter((x) => x.id !== t.id))}>{t.text}</div>
  {/each}
</div>

<style>
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .menu {
    position: fixed;
    z-index: 91;
    min-width: 200px;
    background: var(--raised);
    border: 1px solid var(--border2);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35);
  }
  .item {
    display: flex;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-radius: 4px;
    padding: 4px 10px;
    gap: 20px;
  }
  .item:hover:not(:disabled) {
    background: var(--accent);
    color: #fff;
  }
  .item.danger {
    color: var(--danger);
  }
  .item.danger:hover {
    background: var(--danger);
    color: #fff;
  }
  .sep {
    height: 1px;
    background: var(--border2);
    margin: 4px 6px;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 80;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 12vh;
  }
  .dialog {
    width: 460px;
    max-width: 92vw;
    background: var(--panel);
    border: 1px solid var(--border2);
    border-radius: 8px;
    padding: 16px 18px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .msg {
    color: var(--fg2);
    white-space: pre-wrap;
  }
  .detail {
    max-height: 220px;
    overflow: auto;
    background: var(--bg2);
    border: 1px solid var(--border2);
    border-radius: 4px;
    padding: 6px 8px;
    margin: 0;
    white-space: pre-wrap;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .lbl {
    color: var(--fg2);
    font-size: 12px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .toasts {
    position: fixed;
    right: 14px;
    bottom: 34px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 440px;
  }
  .toast {
    background: var(--raised);
    border: 1px solid var(--border2);
    border-left: 4px solid var(--accent);
    border-radius: 6px;
    padding: 8px 12px;
    white-space: pre-wrap;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.3);
    max-height: 240px;
    overflow: auto;
    font-size: 12px;
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .toast.success {
    border-left-color: var(--ok);
  }
</style>
