import { api, type BranchInfo, type Commit, type Worktree, type WorktreeMode } from './api'
import {
  app, op, dialog, confirm, promptText, repo, short, toast, repoName, samePath, openWorktree, forgetRepoPath, saveConfig,
  type MenuItem,
} from './store.svelte'

export async function fetchAll() {
  await op('Fetch', (r) => api.fetch(r), { success: 'Fetched all remotes' })
}

export async function pull() {
  const rebase = app.config.settings.pullRebase
  await op(rebase ? 'Pull (rebase)' : 'Pull (merge)', (r) => api.pull(r, rebase), { success: 'Pulled' })
}

export async function pullDialog() {
  const r = await dialog({
    title: 'Update / Pull',
    fields: [
      {
        key: 'mode',
        label: 'Strategy',
        type: 'select',
        value: app.config.settings.pullRebase ? 'rebase' : 'merge',
        options: [
          { value: 'merge', label: 'Merge incoming changes' },
          { value: 'rebase', label: 'Rebase onto incoming changes' },
        ],
      },
    ],
    ok: 'Pull',
  })
  if (!r) return
  await op('Pull', (x) => api.pull(x, r.mode === 'rebase'), { success: 'Pulled' })
}

/**
 * Pull from a branch's context menu: the current branch pulls normally, another local branch is
 * fast-forwarded from its upstream without checkout, a remote branch is pulled into the current one.
 */
export async function pullBranch(b: BranchInfo) {
  const rebase = app.config.settings.pullRebase
  const how = rebase ? 'rebase' : 'merge'
  if (b.kind === 'remote') {
    const cur = app.status?.branch ?? 'HEAD'
    return op(`Pull ${b.name} into ${cur} (${how})`, (r) => api.pullBranch(r, b.name, 'remote', rebase), { success: `Pulled ${b.name} into ${cur}` })
  }
  if (b.current) return pull()
  // git won't update a branch that's checked out elsewhere — pull in that worktree instead
  const wt = otherWorktree(b)
  if (wt) return op(`Pull ${b.name} in ${repoName(wt)} (${how})`, () => api.pull(wt, rebase), { success: `Pulled ${b.name}` })
  await op(`Update ${b.name}`, (r) => api.pullBranch(r, b.name, 'local', rebase), { success: `Updated ${b.name} from ${b.upstream}` })
}

/** IDEA-style push dialog: shows outgoing commits before pushing. */
export async function pushDialog(branch: string | null = null, presetForce = false) {
  const r = repo()
  if (!r) return
  const st = app.status
  const br = branch ?? st?.branch
  if (!br) return toast('error', 'HEAD is detached — create a branch before pushing.')
  const info = app.branches.find((b) => b.kind === 'local' && b.name === br)
  let detail = ''
  let message = ''
  if (info?.upstream && !info.gone) {
    try {
      const commits: Commit[] = await api.log(r, { branch: `${info.upstream}..${br}`, skip: 0, limit: 200 })
      if (!commits.length) message = `Nothing to push — ${br} is up to date with ${info.upstream}.`
      else {
        message = `${commits.length} commit${commits.length > 1 ? 's' : ''}: ${br} → ${info.upstream}`
        detail = commits.map((c) => `${short(c.hash)}  ${c.subject}`).join('\n')
      }
      if (info.behind) message += `\n⚠ The remote has ${info.behind} commit(s) you don't have. Pull first, or force push.`
    } catch (e) {
      message = String(e)
    }
  } else {
    const remotes = await api.remotes(r).catch(() => [] as string[])
    const remote = remotes.includes('origin') ? 'origin' : remotes[0]
    if (!remote) return toast('error', 'No remote configured for this repository.')
    message = `New branch: ${br} will be published to ${remote}/${br} and set as upstream.`
  }
  const res = await dialog({
    title: `Push ${br}`,
    message,
    detail: detail || undefined,
    fields: [
      { key: 'force', label: 'Force push (--force-with-lease)', type: 'checkbox', value: presetForce },
      { key: 'tags', label: 'Push annotated tags (--follow-tags)', type: 'checkbox', value: false },
    ],
    ok: 'Push',
  })
  if (!res) return
  if (res.force && !(await confirm('Force Push', `Force push ${br}? Remote commits not in your branch will be lost (protected by --force-with-lease).`, 'Force Push', true))) return
  await op(`Push ${br}`, (x) => api.push(x, branch, !!res.force, !!res.tags), { success: `Pushed ${br}` })
}

/** The other worktree a local branch is checked out in, if any. */
export function otherWorktree(b: { kind: string; worktree?: string | null }) {
  const r = repo()
  return b.kind === 'local' && b.worktree && r && !samePath(b.worktree, r) ? b.worktree : null
}

export async function checkout(b: { name: string; kind: string; worktree?: string | null }) {
  if (b.kind === 'remote') {
    const local = app.branches.find((x) => x.kind === 'local' && x.name === b.name.split('/').slice(1).join('/'))
    if (local && otherWorktree(local)) return checkout(local)
  }
  const wt = otherWorktree(b)
  if (wt) {
    if (await confirm('Branch is checked out in another worktree', `"${b.name}" is already checked out in\n${wt}\n\nGit allows a branch in only one worktree at a time. Switch to that worktree instead?`, 'Open Worktree'))
      await openWorktree(wt)
    return
  }
  await op(`Checkout ${b.name}`, (r) => api.checkout(r, b.name, b.kind), { success: `Checked out ${b.name}` })
}

export async function newBranch(start = '', startLabel = '') {
  const r = await dialog({
    title: startLabel ? `New Branch from ${startLabel}` : 'New Branch',
    fields: [
      { key: 'name', label: 'Branch name', type: 'text', placeholder: 'feature/…' },
      { key: 'checkout', label: 'Checkout branch', type: 'checkbox', value: true },
    ],
    ok: 'Create',
  })
  const name = r?.name?.trim()
  if (!name) return
  await op(`New branch ${name}`, (x) => api.createBranch(x, name, start, !!r!.checkout), { success: `Created ${name}` })
}

export async function checkoutRevision() {
  const rev = await promptText('Checkout Tag or Revision', 'Tag, hash or ref', '', 'Checkout')
  if (rev) await op(`Checkout ${rev}`, (r) => api.checkout(r, rev, 'commit'))
}

export async function deleteBranch(b: BranchInfo) {
  const what = b.kind === 'remote' ? 'remote branch' : b.kind === 'tag' ? 'tag' : 'branch'
  if (!(await confirm(`Delete ${what}`, `Delete ${what} "${b.name}"?${b.kind === 'remote' ? '\nThis deletes it on the server.' : ''}`, 'Delete', true))) return
  // git messages are forced to English (LC_MESSAGES=C), so matching the text is reliable
  let notMerged = false
  await op(`Delete ${b.name}`, (r) => api.deleteBranch(r, b.name, b.kind, false), {
    handled: (e) => (notMerged = b.kind === 'local' && /not fully merged/i.test(e)),
  })
  if (notMerged) {
    if (await confirm('Branch not fully merged', `"${b.name}" is not fully merged. Delete anyway? Its unmerged commits will only be reachable via reflog.`, 'Force Delete', true))
      await op(`Force delete ${b.name}`, (r) => api.deleteBranch(r, b.name, b.kind, true))
  }
}

export async function renameBranch(b: BranchInfo) {
  const n = await promptText('Rename Branch', 'New name', b.name, 'Rename')
  if (n && n !== b.name) await op(`Rename ${b.name}`, (r) => api.renameBranch(r, b.name, n))
}

export async function mergeInto(rev: string) {
  const cur = app.status?.branch ?? 'HEAD'
  if (!(await confirm('Merge', `Merge ${rev} into ${cur}?`, 'Merge'))) return
  await op(`Merge ${rev}`, (r) => api.merge(r, rev), { success: `Merged ${rev}` })
}

export async function rebaseOnto(rev: string) {
  const cur = app.status?.branch ?? 'HEAD'
  if (!(await confirm('Rebase', `Rebase ${cur} onto ${rev}?`, 'Rebase'))) return
  await op(`Rebase onto ${rev}`, (r) => api.rebase(r, rev), { success: `Rebased onto ${rev}` })
}

export async function newTag(rev: string, label: string) {
  const r = await dialog({
    title: `New Tag on ${label}`,
    fields: [
      { key: 'name', label: 'Tag name', type: 'text', placeholder: 'v1.0.0' },
      { key: 'message', label: 'Message (optional — makes an annotated tag)', type: 'text' },
    ],
    ok: 'Create Tag',
  })
  const name = r?.name?.trim()
  if (name) await op(`Tag ${name}`, (x) => api.createTag(x, name, rev, r!.message ?? ''))
}

export async function resetTo(hash: string) {
  const cur = app.status?.branch ?? 'HEAD'
  const r = await dialog({
    title: `Reset ${cur} to ${short(hash)}`,
    fields: [
      {
        key: 'mode',
        label: 'Mode',
        type: 'select',
        value: 'mixed',
        options: [
          { value: 'soft', label: 'Soft — keep changes staged' },
          { value: 'mixed', label: 'Mixed — keep changes in working tree (unstaged)' },
          { value: 'keep', label: 'Keep — reset, but abort if local changes would be lost' },
          { value: 'hard', label: 'Hard — discard all changes (destructive)' },
        ],
      },
    ],
    ok: 'Reset',
    danger: true,
  })
  if (r) await op(`Reset (${r.mode})`, (x) => api.reset(x, hash, r.mode))
}

/**
 * Squash commits of the current branch into one. `commits` newest first (Log order);
 * they don't have to be adjacent (later ones are moved up to the oldest, like IntelliJ).
 */
export async function squashCommits(commits: Commit[]) {
  const r = repo()
  if (!r || commits.length < 2) return
  if (commits.some((c) => c.parents.length > 1)) return toast('error', "Merge commits can't be squashed.")
  const oldestFirst = [...commits].reverse()
  let messages: string[]
  try {
    messages = await Promise.all(oldestFirst.map((c) => api.commitDetail(r, c.hash).then((d) => d.message)))
  } catch (e) {
    return toast('error', String(e))
  }
  let warning = ''
  const st = app.status
  if (st?.upstream) {
    const unpushed = await api.log(r, { branch: `${st.upstream}..HEAD`, skip: 0, limit: 0 }).catch(() => null)
    const set = new Set(unpushed?.map((c) => c.hash))
    if (unpushed && commits.some((c) => !set.has(c.hash)))
      warning = `\n⚠ Some of these commits are already pushed to ${st.upstream}. After squashing you'll need to force push.`
  }
  const res = await dialog({
    title: `Squash ${commits.length} Commits`,
    message: `The commits are combined into one at the position of the oldest. Commits in between are replayed after it.${warning}`,
    detail: oldestFirst.map((c) => `${short(c.hash)}  ${c.subject}`).join('\n'),
    fields: [{ key: 'message', label: 'Commit message', type: 'textarea', value: messages.join('\n\n') }],
    ok: 'Squash',
  })
  if (!res) return
  const message = String(res.message ?? '')
  if (!message.trim()) return toast('error', 'The commit message is empty.')
  await op(`Squash ${commits.length} commits`, (x) => api.squash(x, commits.map((c) => c.hash), message), {
    success: `Squashed ${commits.length} commits`,
  })
}

export async function sequencer(action: 'continue' | 'abort' | 'skip') {
  const kind = app.status?.state.kind
  const opName = kind === 'rebasing' ? 'rebase' : kind === 'merging' ? 'merge' : kind === 'cherry-picking' ? 'cherry-pick' : kind === 'reverting' ? 'revert' : ''
  if (!opName) return
  if (action === 'abort' && !(await confirm(`Abort ${opName}`, `Abort the ${opName} and return to the state before it started?`, 'Abort', true))) return
  await op(`${opName} --${action}`, (r) => api.sequencer(r, opName, action))
}

export async function stashChanges() {
  const r = await dialog({
    title: 'Stash Changes',
    fields: [
      { key: 'message', label: 'Message', type: 'text' },
      { key: 'untracked', label: 'Include untracked files', type: 'checkbox', value: true },
    ],
    ok: 'Stash',
  })
  if (r) await op('Stash', (x) => api.stashPush(x, r.message ?? '', !!r.untracked), { success: 'Changes stashed' })
}

/** Context-menu items for a branch / tag, shared by the Log branch tree and Branches popup. */
export function branchMenuItems(b: BranchInfo, extra: (MenuItem | '-')[] = []): (MenuItem | '-')[] {
  const cur = app.status?.branch
  const isCur = b.kind === 'local' && b.current
  const items: (MenuItem | '-')[] = []
  if (!isCur) items.push({ label: 'Checkout', action: () => checkout(b) })
  items.push({ label: `New Branch from '${b.name}'…`, action: () => newBranch(b.name, b.name) })
  if (!isCur && cur) {
    items.push(
      '-',
      { label: `Merge '${b.name}' into '${cur}'`, action: () => mergeInto(b.name) },
      { label: `Rebase '${cur}' onto '${b.name}'`, action: () => rebaseOnto(b.name) },
    )
  }
  const wt = otherWorktree(b)
  if (wt) items.unshift({ label: `Open Worktree '${repoName(wt)}'`, action: () => openWorktree(wt) }, '-')
  if (b.kind !== 'tag' && !isCur && !wt) items.push({ label: 'Checkout in New Worktree…', action: () => newWorktree(b) })
  if (b.kind === 'remote' && cur) {
    items.push('-', { label: `Pull '${b.name}' into '${cur}'`, action: () => pullBranch(b) })
  }
  if (b.kind === 'local') {
    items.push('-')
    if (b.upstream && !b.gone)
      items.push({ label: isCur || otherWorktree(b) ? 'Pull' : 'Pull (fast-forward only)', action: () => pullBranch(b) })
    items.push({ label: 'Push…', action: () => pushDialog(b.name) }, { label: 'Rename…', action: () => renameBranch(b) })
  }
  items.push(...extra)
  if (!isCur && !wt) items.push('-', { label: 'Delete', danger: true, action: () => deleteBranch(b) })
  return items
}

// ───────── worktrees ─────────

const join = (dir: string, name: string) => dir.replace(/[\\/]+$/, '') + (dir.includes('\\') && !dir.includes('/') ? '\\' : '/') + name
const parentDir = (p: string) => p.replace(/[\\/]+$/, '').replace(/[\\/][^\\/]*$/, '')
const slug = (s: string) => s.replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '')

async function mainWorktree(r: string) {
  const list = await api.worktreeList(r).catch(() => [] as Worktree[])
  return list.find((w) => w.main)?.path ?? r
}

/**
 * New worktree dialog. With `from` (a branch or tag) the matching mode is preselected;
 * a commit hash can be passed as `{ name: hash, kind: 'commit' }`.
 */
export async function newWorktree(from?: { name: string; kind: string }) {
  const r = repo()
  if (!r) return
  const main = await mainWorktree(r)
  const base = repoName(main)
  const kind = from?.kind
  const mode: WorktreeMode = !from ? 'new' : kind === 'local' ? 'existing' : kind === 'remote' ? 'remote' : 'detached'
  const local = kind === 'remote' ? from!.name.split('/').slice(1).join('/') : (from?.name ?? '')
  const branchExists = (n: string) => app.branches.some((b) => b.kind === 'local' && b.name === n)
  const res = await dialog({
    title: from ? `New Worktree from ${kind === 'commit' ? short(from.name) : from.name}` : 'New Worktree',
    message: 'Creates a second working directory for this repository, so you can work on another branch without stashing or switching.',
    fields: [
      {
        key: 'mode',
        label: 'Branch',
        type: 'select',
        value: mode === 'remote' && branchExists(local) ? 'existing' : mode,
        options: [
          { value: 'new', label: 'Create a new branch' },
          { value: 'existing', label: 'Check out an existing local branch' },
          { value: 'remote', label: 'Track a remote branch (creates local branch)' },
          { value: 'detached', label: 'Detached HEAD at a revision' },
        ],
      },
      { key: 'branch', label: 'Branch name (new / existing / local tracking branch)', type: 'text', value: mode === 'detached' ? '' : local, placeholder: 'feature/…' },
      { key: 'start', label: 'Start point (for new / remote / detached; empty = HEAD)', type: 'text', value: mode === 'existing' ? '' : (from?.name ?? '') },
      { key: 'path', label: 'Folder (empty = next to the main worktree, named after the branch)', type: 'text', placeholder: join(parentDir(main), `${base}-…`) },
      { key: 'open', label: 'Open the new worktree in Stashwick', type: 'checkbox', value: true },
    ],
    ok: 'Create Worktree',
  })
  if (!res) return
  const m = res.mode as WorktreeMode
  const branch = String(res.branch ?? '').trim()
  const start = String(res.start ?? '').trim()
  if (m !== 'detached' && !branch) return toast('error', 'Enter a branch name.')
  if (m === 'remote' && !start) return toast('error', 'Enter the remote branch (e.g. origin/feature) as start point.')
  const tail = slug(m === 'detached' ? (start ? short(start) : 'detached') : branch) || 'worktree'
  const path = String(res.path ?? '').trim() || join(parentDir(main), `${base}-${tail}`)
  const out = await op(`Add worktree ${repoName(path)}`, (x) => api.worktreeAdd(x, path, m, branch, start), { success: `Worktree created: ${path}` })
  if (out !== undefined && res.open) await openWorktree(path)
}

export async function removeWorktree(w: Worktree) {
  const r = repo()
  if (!r || w.main) return
  const label = repoName(w.path)
  if (w.locked !== null) return toast('error', `Worktree "${label}" is locked${w.locked ? ` (${w.locked})` : ''}. Unlock it first.`)
  if (!(await confirm('Remove Worktree', `Delete the worktree folder\n${w.path}\n\nThe branch${w.branch ? ` "${w.branch}"` : ''} and its commits are kept.`, 'Remove', true))) return
  // run from the main worktree (and step out of it first), so removing the worktree you are in also works
  const main = await mainWorktree(r)
  if (samePath(r, w.path)) await openWorktree(main, r)
  let dirty = false
  let ok = await op(`Remove worktree ${label}`, () => api.worktreeAction(main, 'remove', w.path), {
    handled: (e) => (dirty = /modified or untracked files/i.test(e)),
  })
  if (dirty) {
    if (!(await confirm('Worktree has local changes', `"${label}" has modified or untracked files. Remove it anyway? Those changes will be lost.`, 'Force Remove', true))) return
    ok = await op(`Force remove worktree ${label}`, () => api.worktreeAction(main, 'force-remove', w.path))
  }
  if (ok !== undefined) {
    forgetRepoPath(w.path, main)
    toast('success', `Removed worktree ${label}`)
  }
}

export async function lockWorktree(w: Worktree) {
  if (w.locked !== null) return op(`Unlock worktree ${repoName(w.path)}`, (r) => api.worktreeAction(r, 'unlock', w.path))
  const reason = await promptText('Lock Worktree', 'Reason (optional) — a locked worktree is never pruned or removed', '', 'Lock')
  if (reason !== null) await op(`Lock worktree ${repoName(w.path)}`, (r) => api.worktreeAction(r, 'lock', w.path, reason))
}

export async function moveWorktree(w: Worktree) {
  const to = await promptText('Move Worktree', 'New folder', w.path, 'Move')
  if (!to || samePath(to, w.path)) return
  const r = repo()!
  const main = await mainWorktree(r)
  const wasActive = samePath(r, w.path)
  if (wasActive) await openWorktree(main, r)
  const ok = await op(`Move worktree ${repoName(w.path)}`, () => api.worktreeAction(main, 'move', w.path, to))
  if (ok === undefined) return
  const dest = await api.openRepo(to).catch(() => to)
  for (const ws of app.config.workspaces) ws.repos = ws.repos.map((p) => (samePath(p, w.path) ? dest : p))
  if (wasActive) await openWorktree(dest, main)
  else saveConfig()
}

export async function pruneWorktrees() {
  await op('Prune worktrees', (r) => api.worktreeAction(r, 'prune', ''), { success: 'Pruned stale worktree entries' })
}
