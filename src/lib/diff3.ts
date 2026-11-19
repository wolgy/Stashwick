/** Line diff (Myers) and 3-way merge used by the conflict resolver. */

export function splitLines(s: string): string[] {
  if (!s) return []
  return s.match(/[^\n]*\n|[^\n]+$/g) ?? []
}

interface Change {
  aStart: number
  aEnd: number
  bStart: number
  bEnd: number
}

/** Matched index pairs (i in a, j in b), ascending. */
function matches(a: string[], b: string[]): [number, number][] {
  let pre = 0
  while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++
  let suf = 0
  while (suf < a.length - pre && suf < b.length - pre && a[a.length - 1 - suf] === b[b.length - 1 - suf]) suf++
  const A = a.slice(pre, a.length - suf)
  const B = b.slice(pre, b.length - suf)
  const out: [number, number][] = []
  for (let i = 0; i < pre; i++) out.push([i, i])
  const mid = myers(A, B)
  for (const [i, j] of mid) out.push([i + pre, j + pre])
  for (let k = suf; k > 0; k--) out.push([a.length - k, b.length - k])
  return out
}

function myers(a: string[], b: string[]): [number, number][] {
  const n = a.length
  const m = b.length
  if (n === 0 || m === 0) return []
  const max = n + m
  const off = max + 1
  const v = new Int32Array(2 * max + 3)
  const trace: Int32Array[] = []
  const LIMIT = 2000 // the trace costs ~LIMIT² ints (16 MB here); beyond that show one big conflict
  let found = -1
  outer: for (let d = 0; d <= max; d++) {
    if (d > LIMIT) return [] // pathological: treat as full replacement
    trace.push(v.slice(off - d - 1, off + d + 2))
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]) ? v[off + k + 1] : v[off + k - 1] + 1
      let y = x - k
      while (x < n && y < m && a[x] === b[y]) {
        x++
        y++
      }
      v[off + k] = x
      if (x >= n && y >= m) {
        found = d
        break outer
      }
    }
  }
  const get = (d: number, k: number) => trace[d][k + d + 1]
  const res: [number, number][] = []
  let x = n
  let y = m
  for (let d = found; d >= 0; d--) {
    const k = x - y
    const prevK = k === -d || (k !== d && get(d, k - 1) < get(d, k + 1)) ? k + 1 : k - 1
    const prevX = d === 0 ? 0 : get(d, prevK)
    const prevY = d === 0 ? 0 : prevX - prevK
    while (x > prevX && y > prevY) {
      x--
      y--
      res.push([x, y])
    }
    x = prevX
    y = prevY
  }
  return res.reverse()
}

export function diffChanges(a: string[], b: string[]): Change[] {
  const ms = matches(a, b)
  const out: Change[] = []
  let ai = 0
  let bi = 0
  for (const [i, j] of [...ms, [a.length, b.length] as [number, number]]) {
    if (i > ai || j > bi) out.push({ aStart: ai, aEnd: i, bStart: bi, bEnd: j })
    ai = i + 1
    bi = j + 1
  }
  return out
}

export type Resolution = 'ours' | 'theirs' | 'ours+theirs' | 'theirs+ours' | 'base' | 'custom' | null

export type Chunk =
  | { kind: 'same'; lines: string[] }
  | { kind: 'auto'; side: 'ours' | 'theirs' | 'both'; base: string[]; lines: string[] }
  | {
      kind: 'conflict'
      base: string[]
      ours: string[]
      theirs: string[]
      resolution: Resolution
      custom: string
    }

export function merge3(baseText: string, oursText: string, theirsText: string): Chunk[] {
  const base = splitLines(baseText)
  const ours = splitLines(oursText)
  const theirs = splitLines(theirsText)
  const changes = [
    ...diffChanges(base, ours).map((c) => ({ ...c, side: 0 })),
    ...diffChanges(base, theirs).map((c) => ({ ...c, side: 1 })),
  ].sort((x, y) => x.aStart - y.aStart || x.side - y.side)

  const chunks: Chunk[] = []
  let pos = 0
  let i = 0
  while (i < changes.length) {
    const group = [changes[i]]
    let s = changes[i].aStart
    let e = changes[i].aEnd
    i++
    while (i < changes.length && changes[i].aStart <= e) {
      // overlapping or touching → same region (conservative, like diff3)
      group.push(changes[i])
      e = Math.max(e, changes[i].aEnd)
      i++
    }
    if (s > pos) chunks.push({ kind: 'same', lines: base.slice(pos, s) })
    const sideRange = (side: number, src: string[]) => {
      const cs = group.filter((c) => c.side === side)
      if (!cs.length) return base.slice(s, e)
      const first = cs[0]
      const last = cs[cs.length - 1]
      return src.slice(first.bStart - (first.aStart - s), last.bEnd + (e - last.aEnd))
    }
    const hasO = group.some((c) => c.side === 0)
    const hasT = group.some((c) => c.side === 1)
    const o = sideRange(0, ours)
    const t = sideRange(1, theirs)
    const b = base.slice(s, e)
    if (hasO && !hasT) chunks.push({ kind: 'auto', side: 'ours', base: b, lines: o })
    else if (hasT && !hasO) chunks.push({ kind: 'auto', side: 'theirs', base: b, lines: t })
    else if (o.join('') === t.join('')) chunks.push({ kind: 'auto', side: 'both', base: b, lines: o })
    else chunks.push({ kind: 'conflict', base: b, ours: o, theirs: t, resolution: null, custom: '' })
    pos = e
  }
  if (pos < base.length) chunks.push({ kind: 'same', lines: base.slice(pos) })
  return chunks
}

/** Terminate the last line, so a following block doesn't get glued onto it. */
const nl = (arr: string[]) => (arr.length && !arr[arr.length - 1].endsWith('\n') ? [...arr.slice(0, -1), arr[arr.length - 1] + '\n'] : arr)

export function resolvedLines(c: Extract<Chunk, { kind: 'conflict' }>, oursLabel = 'ours', theirsLabel = 'theirs'): string[] {
  switch (c.resolution) {
    case 'ours':
      return c.ours
    case 'theirs':
      return c.theirs
    case 'ours+theirs':
      return [...nl(c.ours), ...c.theirs]
    case 'theirs+ours':
      return [...nl(c.theirs), ...c.ours]
    case 'base':
      return c.base
    case 'custom':
      return splitLines(c.custom)
    default: {
      return [`<<<<<<< ${oursLabel}\n`, ...nl(c.ours), '=======\n', ...nl(c.theirs), `>>>>>>> ${theirsLabel}\n`]
    }
  }
}

export function buildResult(chunks: Chunk[], oursLabel?: string, theirsLabel?: string): string {
  return chunks
    .map((c) => (c.kind === 'conflict' ? resolvedLines(c, oursLabel, theirsLabel) : c.lines).join(''))
    .join('')
}
