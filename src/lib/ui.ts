/** Svelte action: drag a splitter. Calls onDelta with pixel movement since drag start. */
export function splitter(node: HTMLElement, params: { onStart: () => void; onDelta: (dx: number, dy: number) => void }) {
  let p = params
  const down = (e: PointerEvent) => {
    e.preventDefault()
    const sx = e.clientX
    const sy = e.clientY
    p.onStart()
    const move = (ev: PointerEvent) => p.onDelta(ev.clientX - sx, ev.clientY - sy)
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      document.body.style.cursor = ''
    }
    document.body.style.cursor = getComputedStyle(node).cursor
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }
  node.addEventListener('pointerdown', down)
  return {
    update(np: typeof params) {
      p = np
    },
    destroy() {
      node.removeEventListener('pointerdown', down)
    },
  }
}

/** Persisted panel size helper. */
export function sizeStore(key: string, initial: number) {
  const k = `stashwick.size.${key}`
  let v = initial
  try {
    const s = localStorage.getItem(k)
    if (s) v = Number(s) || initial
  } catch {
    /* storage unavailable */
  }
  return {
    get: () => v,
    set: (n: number) => {
      v = n
      try {
        localStorage.setItem(k, String(n))
      } catch {
        /* ignore */
      }
    },
  }
}

export async function copyText(t: string) {
  try {
    await navigator.clipboard.writeText(t)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = t
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    ta.remove()
  }
}

export function splitPath(p: string) {
  const i = p.lastIndexOf('/')
  return i === -1 ? { dir: '', name: p } : { dir: p.slice(0, i), name: p.slice(i + 1) }
}
