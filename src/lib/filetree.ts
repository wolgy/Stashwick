/** Rows for showing a flat file list grouped by directory, IDEA style. */
export type TreeRow<T> =
  | { kind: 'dir'; key: string; dir: string; name: string; depth: number; count: number }
  | { kind: 'file'; key: string; item: T; depth: number }

interface Node<T> {
  dirs: Map<string, Node<T>>
  files: { name: string; item: T }[]
  count: number
}

const newNode = <T>(): Node<T> => ({ dirs: new Map(), files: [], count: 0 })
const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' })

/**
 * Directories first, then files, both sorted by name. A directory whose only
 * content is one subdirectory is merged with it (`src/lib`). Contents of a
 * directory for which `collapsed(key)` is true are left out. `prefix` keeps keys
 * unique when the same folder appears in several lists (e.g. Staged and Changes).
 */
export function fileTree<T>(items: T[], pathOf: (t: T) => string, collapsed: (key: string) => boolean, prefix = ''): TreeRow<T>[] {
  const root = newNode<T>()
  for (const item of items) {
    const parts = pathOf(item).split('/')
    const name = parts.pop()!
    let n = root
    n.count++
    for (const p of parts) {
      let c = n.dirs.get(p)
      if (!c) n.dirs.set(p, (c = newNode()))
      n = c
      n.count++
    }
    n.files.push({ name, item })
  }

  const rows: TreeRow<T>[] = []
  const walk = (n: Node<T>, base: string, depth: number) => {
    for (const name of [...n.dirs.keys()].sort(byName)) {
      let label = name
      let node = n.dirs.get(name)!
      while (node.files.length === 0 && node.dirs.size === 1) {
        const [sub, child] = [...node.dirs][0]
        label += '/' + sub
        node = child
      }
      const dir = base ? `${base}/${label}` : label
      const key = prefix + dir
      rows.push({ kind: 'dir', key, dir, name: label, depth, count: node.count })
      if (!collapsed(key)) walk(node, dir, depth + 1)
    }
    for (const f of n.files.sort((a, b) => byName(a.name, b.name))) {
      rows.push({ kind: 'file', key: prefix + (base ? `${base}/${f.name}` : f.name), item: f.item, depth })
    }
  }
  walk(root, '', 0)
  return rows
}

/** Items whose path is inside `dir`. */
export const inDir = <T>(items: T[], pathOf: (t: T) => string, dir: string) => items.filter((t) => pathOf(t).startsWith(dir + '/'))
