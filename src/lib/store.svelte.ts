import { api, type BranchInfo, type RepoStatus, type RepoSummary } from './api'

export interface Workspace {
  id: string
  name: string
  repos: string[]
}
export interface Settings {
  gitPath: string
  editor: string
  pullRebase: boolean
  logPageSize: number
  theme: 'auto' | 'dark' | 'light'
}
export interface Config {
  workspaces: Workspace[]
  activeWorkspace: string
  activeRepo: string | null
  lastRepoByWorkspace: Record<string, string>
  settings: Settings
}

const defaultConfig = (): Config => ({
  workspaces: [{ id: 'default', name: 'Default', repos: [] }],
  activeWorkspace: 'default',
  activeRepo: null,
  lastRepoByWorkspace: {},
  settings: { gitPath: '', editor: 'zed', pullRebase: false, logPageSize: 1000, theme: 'auto' },
})

export interface ConsoleEntry {
  time: number
  title: string
  output: string
  ok: boolean
  repo: string
}
export interface Toast {
  id: number
  kind: 'info' | 'error' | 'success'
  text: string
}

export type Field =
  | { key: string; label: string; type: 'text' | 'textarea'; value?: string; placeholder?: string }
  | { key: string; label: string; type: 'checkbox'; value?: boolean }
  | { key: string; label: string; type: 'select'; value?: string; options: { value: string; label: string }[] }

export interface DialogSpec {
  title: string
  message?: string
  detail?: string
  fields?: Field[]
  ok?: string
  cancel?: string | null
  danger?: boolean
  resolve: (v: Record<string, any> | null) => void
}

export interface MenuItem {
  label: string
  action?: () => void
  danger?: boolean
  disabled?: boolean
  shortcut?: string
}

export const app = $state({
  config: defaultConfig(),
  ready: false,
  summaries: {} as Record<string, RepoSummary>,
  status: null as RepoStatus | null,
  branches: [] as BranchInfo[],
  busy: null as string | null,
  toasts: [] as Toast[],
  console: [] as ConsoleEntry[],
  consoleOpen: false,
  view: 'commit' as 'commit' | 'log',
  /** bump to make the log reload */
  /** bumped after every refresh (stash list, worktrees…) */
  logToken: 0,
  /** bumped only when HEAD or a ref moved — drives the (expensive) log reload */
  refsToken: 0,
  dialog: null as DialogSpec | null,
  menu: null as { x: number; y: number; items: (MenuItem | '-')[] } | null,
  mergeFile: null as string | null,
  branchesOpen: false,
  settingsOpen: false,
  stashOpen: false,
  worktreesOpen: false,
  logFilter: { branch: null as string | null, path: '' as string },
})

export const repo = () => app.config.activeRepo
export const workspace = () => app.config.workspaces.find((w) => w.id === app.config.activeWorkspace) ?? app.config.workspaces[0]
export const repoName = (p: string) => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p

// ───────── config persistence ─────────
let saveTimer: ReturnType<typeof setTimeout> | undefined
export function saveConfig() {
  clearTimeout(saveTimer)
  saveTimer = setTimeout(() => api.saveConfig($state.snapshot(app.config)).catch((e) => toast('error', String(e))), 300)
}

export async function initApp() {
  try {
    const c = (await api.loadConfig()) as Partial<Config> | null
    if (c && typeof c === 'object') {
      const d = defaultConfig()
      app.config = { ...d, ...c, settings: { ...d.settings, ...(c.settings ?? {}) } }
      if (!app.config.workspaces.length) app.config.workspaces = d.workspaces
    }
  } catch (e) {
    toast('error', `Could not load config: ${e}`)
  }
  await api.setGitPath(app.config.settings.gitPath)
  applyTheme()
  app.ready = true
  await refreshAll()
}

export function applyTheme() {
  document.documentElement.dataset.theme = app.config.settings.theme
}

// ───────── toasts / console ─────────
let toastId = 0
export function toast(kind: Toast['kind'], text: string, ms = kind === 'error' ? 9000 : 3500) {
  const id = ++toastId
  app.toasts.push({ id, kind, text })
  setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), ms)
}

function log(title: string, output: string, ok: boolean) {
  app.console.unshift({ time: Date.now(), title, output, ok, repo: repo() ?? '' })
  if (app.console.length > 200) app.console.length = 200
}

// ───────── dialogs / menus ─────────
export function dialog(spec: Omit<DialogSpec, 'resolve'>): Promise<Record<string, any> | null> {
  return new Promise((resolve) => {
    app.dialog = { ...spec, resolve }
  })
}
export async function confirm(title: string, message: string, ok = 'OK', danger = false, detail?: string) {
  return (await dialog({ title, message, ok, danger, detail })) !== null
}
export async function promptText(title: string, label: string, value = '', ok = 'OK') {
  const r = await dialog({ title, fields: [{ key: 'v', label, type: 'text', value }], ok })
  return r ? String(r.v ?? '').trim() : null
}
export function showMenu(e: MouseEvent, items: (MenuItem | '-')[]) {
  e.preventDefault()
  e.stopPropagation()
  app.menu = { x: e.clientX, y: e.clientY, items }
}

// ───────── refresh ─────────
// per repo, so switching repos mid-refresh doesn't skip the new repo's status
const refreshing = new Set<string>()
export async function refreshStatus() {
  const r = repo()
  if (!r || refreshing.has(r)) return
  refreshing.add(r)
  try {
    const st = await api.status(r)
    if (repo() === r) app.status = st
  } catch (e) {
    if (repo() === r) app.status = null
    toast('error', String(e))
  } finally {
    refreshing.delete(r)
  }
}

async function refreshBranches() {
  const r = repo()
  if (!r) return
  try {
    const b = await api.branches(r)
    if (repo() === r) app.branches = b
  } catch (e) {
    console.error('branches:', e)
    if (repo() === r) app.branches = []
  }
}

let summariesBusy = false
export async function refreshSummaries() {
  const ws = workspace()
  if (!ws?.repos.length || summariesBusy) return
  summariesBusy = true
  try {
    const s = await api.summaries(ws.repos)
    for (const x of s) app.summaries[x.path] = x
  } catch (e) {
    console.error('summaries:', e)
  } finally {
    summariesBusy = false
  }
}

let lastRefsKey = ''
export async function refreshAll() {
  await Promise.all([refreshStatus(), refreshBranches(), refreshSummaries()])
  app.logToken++
  const key = `${app.config.activeRepo}\0${app.status?.head ?? ''}\0${app.branches.map((b) => b.full + ':' + b.hash).join(',')}`
  if (key !== lastRefsKey) {
    lastRefsKey = key
    app.refsToken++
  }
}

// ───────── operations ─────────
/** Run a git operation with busy indicator, console log, toast on error and refresh. */
/**
 * `handled`: called with the error text; return true when the caller deals with
 * this specific failure itself (no error toast), e.g. to offer a force variant.
 */
export async function op<T>(
  title: string,
  fn: (repo: string) => Promise<T>,
  opts: { success?: string; quiet?: boolean; handled?: (err: string) => boolean } = {},
) {
  const r = repo()
  if (!r) return undefined
  app.busy = title
  try {
    const res = await fn(r)
    log(title, typeof res === 'string' ? res : '', true)
    if (opts.success) toast('success', opts.success)
    return res
  } catch (e) {
    const msg = String(e)
    log(title, msg, false)
    if (!opts.handled?.(msg)) toast('error', `${title} failed:\n${msg}`)
    return undefined
  } finally {
    app.busy = null
    await refreshAll()
  }
}

// ───────── workspace / repo management ─────────
export function selectRepo(path: string) {
  if (app.config.activeRepo === path) return
  app.config.activeRepo = path
  app.config.lastRepoByWorkspace[app.config.activeWorkspace] = path
  app.status = null
  app.branches = []
  app.logFilter = { branch: null, path: '' }
  saveConfig()
  refreshAll()
}

export function selectWorkspace(id: string) {
  app.config.activeWorkspace = id
  const ws = workspace()
  const last = app.config.lastRepoByWorkspace[id]
  app.config.activeRepo = ws.repos.includes(last) ? last : (ws.repos[0] ?? null)
  app.status = null
  app.branches = []
  saveConfig()
  refreshAll()
}

export async function addRepoPaths(paths: string[]) {
  const ws = workspace()
  let added = 0
  for (const p of paths) {
    try {
      const top = await api.openRepo(p)
      if (!ws.repos.includes(top)) {
        ws.repos.push(top)
        added++
      }
      if (!app.config.activeRepo) app.config.activeRepo = top
    } catch (e) {
      toast('error', String(e))
    }
  }
  saveConfig()
  if (added) toast('success', `Added ${added} repositor${added === 1 ? 'y' : 'ies'} to ${ws.name}`)
  await refreshAll()
}

export function removeRepo(path: string) {
  const ws = workspace()
  ws.repos = ws.repos.filter((r) => r !== path)
  if (app.config.activeRepo === path) app.config.activeRepo = ws.repos[0] ?? null
  saveConfig()
  refreshAll()
}

/** Same path, ignoring trailing separators and separator style. */
export const samePath = (a: string, b: string) => {
  const n = (p: string) => p.replace(/[\\/]+$/, '').replace(/\\/g, '/')
  return n(a) === n(b)
}

/** Make a worktree (or any repo path) active: add it to the workspace right after `near` if missing. */
export async function openWorktree(path: string, near: string | null = repo()) {
  const ws = workspace()
  let top = path
  try {
    top = await api.openRepo(path)
  } catch (e) {
    return toast('error', String(e))
  }
  const existing = ws.repos.find((r) => samePath(r, top))
  if (existing) return selectRepo(existing)
  const i = near ? ws.repos.indexOf(near) : -1
  if (i >= 0) ws.repos.splice(i + 1, 0, top)
  else ws.repos.push(top)
  selectRepo(top)
}

/** Drop a path from every workspace (used after a worktree was deleted on disk). */
export function forgetRepoPath(path: string, fallback: string | null) {
  for (const w of app.config.workspaces) w.repos = w.repos.filter((r) => !samePath(r, path))
  if (app.config.activeRepo && samePath(app.config.activeRepo, path)) {
    const ws = workspace()
    app.config.activeRepo = null
    const next = fallback && ws.repos.find((r) => samePath(r, fallback))
    if (next) selectRepo(next)
    else if (ws.repos[0]) selectRepo(ws.repos[0])
  }
  saveConfig()
}

export function newWorkspace(name: string) {
  const id = Math.random().toString(36).slice(2, 10)
  app.config.workspaces.push({ id, name, repos: [] })
  selectWorkspace(id)
}

export async function openInEditor(paths: string[]) {
  const r = repo()
  if (!r) return
  try {
    await api.openInEditor(app.config.settings.editor, paths, r)
  } catch (e) {
    toast('error', String(e))
  }
}

// ───────── formatting ─────────
export function fmtDate(unix: number) {
  const d = new Date(unix * 1000)
  const now = new Date()
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString()
  if (sameDay(d, now)) return `Today ${time}`
  const y = new Date(now)
  y.setDate(now.getDate() - 1)
  if (sameDay(d, y)) return `Yesterday ${time}`
  return d.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' }) + ' ' + time
}
export const short = (h: string) => h.slice(0, 8)
const isMac = navigator.platform.toLowerCase().includes('mac')
export const mod = (e: KeyboardEvent | MouseEvent) => (isMac ? e.metaKey : e.ctrlKey)
export const modLabel = isMac ? '⌘' : 'Ctrl+'
