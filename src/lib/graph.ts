import type { Commit } from './api'

export interface Edge {
  from: number
  to: number
  color: number
}
export interface GraphRow {
  col: number
  color: number
  /** lane at top → node */
  incoming: { from: number; color: number }[]
  /** node → lane at bottom */
  outgoing: { to: number; color: number }[]
  /** lanes passing straight through */
  passes: Edge[]
  width: number
}

/** Lane state carried between pages, so appending a page only lays out the new rows. */
export interface GraphState {
  lanes: (string | null)[]
  colors: number[]
  nextColor: number
}
export const newGraphState = (): GraphState => ({ lanes: [], colors: [], nextColor: 0 })

/** Color index for commits (and the lines below them) that are already in main/master. */
export const MAIN = -1

/**
 * Assign commits (topo-ordered, newest first) to lanes, IDEA/gitk style.
 * Pass the state returned for the previous page to continue where it ended.
 */
export function layoutGraph(commits: Commit[], state: GraphState = newGraphState()): GraphRow[] {
  const { lanes, colors } = state
  const rows: GraphRow[] = []
  const firstFree = () => {
    const i = lanes.indexOf(null)
    return i === -1 ? lanes.length : i
  }

  for (const c of commits) {
    const top = lanes.slice()
    const topColors = colors.slice()
    let col = lanes.indexOf(c.hash)
    if (col === -1) {
      col = firstFree()
      lanes[col] = c.hash
      colors[col] = c.inMain ? MAIN : state.nextColor++
    }
    // ancestors of a main commit are in main too, so everything below it is MAIN
    const color = c.inMain ? MAIN : colors[col]
    const incoming: GraphRow['incoming'] = []
    top.forEach((h, i) => {
      if (h === c.hash) incoming.push({ from: i, color: topColors[i] })
    })
    for (let i = 0; i < lanes.length; i++) if (lanes[i] === c.hash) lanes[i] = null

    const outgoing: GraphRow['outgoing'] = []
    c.parents.forEach((p, k) => {
      let idx = lanes.indexOf(p)
      if (k === 0 && idx > col) {
        // keep the first-parent line straight; lanes converge at the parent
        lanes[col] = p
        colors[col] = color
        idx = col
      } else if (idx === -1) {
        if (k === 0) {
          idx = col
          colors[idx] = color
        } else {
          idx = firstFree()
          colors[idx] = c.inMain ? MAIN : state.nextColor++
        }
        lanes[idx] = p
      }
      outgoing.push({ to: idx, color: c.inMain ? MAIN : colors[idx] })
    })
    while (lanes.length && lanes[lanes.length - 1] === null) {
      lanes.pop()
      colors.pop()
    }
    const passes: Edge[] = []
    top.forEach((h, i) => {
      if (h && h !== c.hash) passes.push({ from: i, to: i, color: topColors[i] })
    })
    rows.push({
      col,
      color,
      incoming,
      outgoing,
      passes,
      width: Math.max(top.length, lanes.length, col + 1),
    })
  }
  return rows
}

const LANE_COLORS = [
  '#4c9aff', '#f5a623', '#36b37e', '#e5556f', '#a77bf3', '#00b8d9', '#ff8b3d', '#8bc34a', '#f06292', '#ffd54f',
]
const MAIN_COLOR = '#8a8f98'
export const laneColor = (i: number) => (i === MAIN ? MAIN_COLOR : LANE_COLORS[i % LANE_COLORS.length])
