import { invoke } from '@tauri-apps/api/core'

export interface FileStatus {
  path: string
  origPath: string | null
  index: string
  worktree: string
  conflict: boolean
  untracked: boolean
}
export interface RepoState {
  kind: '' | 'merging' | 'rebasing' | 'cherry-picking' | 'reverting'
  detail: string | null
  oursLabel: string
  theirsLabel: string
}
export interface RepoStatus {
  root: string
  branch: string | null
  head: string | null
  upstream: string | null
  ahead: number
  behind: number
  files: FileStatus[]
  state: RepoState
}
export interface RepoSummary {
  path: string
  branch: string | null
  ahead: number
  behind: number
  changes: number
  conflicts: number
  state: string
  error: string | null
  linked: boolean
}
export interface DiffLine {
  kind: ' ' | '+' | '-' | '\\'
  text: string
  oldNo: number | null
  newNo: number | null
}
export interface Hunk {
  header: string
  oldStart: number
  oldCount: number
  newStart: number
  newCount: number
  lines: DiffLine[]
}
export interface FileDiff {
  header: string[]
  hunks: Hunk[]
  binary: boolean
  tooLarge: boolean
  untracked: boolean
  fingerprint: string
  note: string | null
}
export interface RefLabel {
  name: string
  kind: 'head' | 'local' | 'remote' | 'tag'
  current: boolean
}
export interface Commit {
  hash: string
  parents: string[]
  author: string
  email: string
  time: number
  subject: string
  refs: RefLabel[]
}
export interface ChangedFile {
  status: string
  path: string
  origPath: string | null
}
export interface CommitDetail {
  hash: string
  parents: string[]
  author: string
  email: string
  authorTime: number
  committer: string
  committerEmail: string
  commitTime: number
  message: string
  files: ChangedFile[]
}
export interface BranchInfo {
  name: string
  full: string
  kind: 'local' | 'remote' | 'tag'
  hash: string
  upstream: string | null
  ahead: number
  behind: number
  gone: boolean
  current: boolean
  time: number
  subject: string
  worktree: string | null
}
export interface Worktree {
  path: string
  head: string | null
  branch: string | null
  bare: boolean
  detached: boolean
  main: boolean
  current: boolean
  locked: string | null
  prunable: string | null
}
export type WorktreeMode = 'new' | 'existing' | 'remote' | 'detached'
export interface StashEntry {
  ref: string
  time: number
  message: string
}
export interface ConflictVersions {
  base: string | null
  ours: string | null
  theirs: string | null
  working: string | null
  binary: boolean
  oursLabel: string
  theirsLabel: string
}
export interface LogQuery {
  branch?: string | null
  author?: string | null
  text?: string | null
  path?: string | null
  skip: number
  limit: number
}

const call = <T>(cmd: string, args: Record<string, unknown> = {}) => invoke<T>(cmd, args)

export const api = {
  status: (repo: string) => call<RepoStatus>('status', { repo }),
  fileDiff: (repo: string, path: string, origPath: string | null, staged: boolean, untracked: boolean) =>
    call<FileDiff>('file_diff', { repo, path, origPath, staged, untracked }),
  applyLines: (
    repo: string,
    path: string,
    origPath: string | null,
    mode: 'stage' | 'unstage' | 'discard',
    fingerprint: string,
    sel: { hunk: number; line: number }[],
  ) => call<string>('apply_lines', { repo, path, origPath, mode, fingerprint, sel }),
  stage: (repo: string, paths: string[]) => call<string>('stage', { repo, paths }),
  unstage: (repo: string, paths: string[]) => call<string>('unstage', { repo, paths }),
  rollback: (repo: string, files: FileStatus[]) => call<string>('rollback', { repo, files }),
  commit: (repo: string, message: string, amend: boolean, signoff: boolean) =>
    call<string>('commit', { repo, message, amend, signoff }),
  lastMessage: (repo: string) => call<string>('last_message', { repo }),
  log: (repo: string, query: LogQuery) => call<Commit[]>('log', { repo, query }),
  commitDetail: (repo: string, hash: string) => call<CommitDetail>('commit_detail', { repo, hash }),
  commitFileDiff: (repo: string, hash: string, path: string, origPath: string | null) =>
    call<FileDiff>('commit_file_diff', { repo, hash, path, origPath }),
  branches: (repo: string) => call<BranchInfo[]>('branches', { repo }),
  remotes: (repo: string) => call<string[]>('remotes', { repo }),
  checkout: (repo: string, name: string, kind: string) => call<string>('checkout', { repo, name, kind }),
  createBranch: (repo: string, name: string, start: string, checkout: boolean) =>
    call<string>('create_branch', { repo, name, start, checkout }),
  deleteBranch: (repo: string, name: string, kind: string, force: boolean) =>
    call<string>('delete_branch', { repo, name, kind, force }),
  renameBranch: (repo: string, old: string, newName: string) => call<string>('rename_branch', { repo, old, new: newName }),
  merge: (repo: string, rev: string) => call<string>('merge', { repo, rev }),
  rebase: (repo: string, onto: string) => call<string>('rebase', { repo, onto }),
  sequencer: (repo: string, op: string, action: string) => call<string>('sequencer', { repo, op, action }),
  cherryPick: (repo: string, hash: string) => call<string>('cherry_pick', { repo, hash }),
  revert: (repo: string, hash: string) => call<string>('revert', { repo, hash }),
  reset: (repo: string, hash: string, mode: string) => call<string>('reset', { repo, hash, mode }),
  createTag: (repo: string, name: string, rev: string, message: string) =>
    call<string>('create_tag', { repo, name, rev, message }),
  fetch: (repo: string) => call<string>('fetch', { repo }),
  pull: (repo: string, rebase: boolean) => call<string>('pull', { repo, rebase }),
  push: (repo: string, branch: string | null, force: boolean, tags: boolean) =>
    call<string>('push', { repo, branch, force, tags }),
  stashList: (repo: string) => call<StashEntry[]>('stash_list', { repo }),
  stashPush: (repo: string, message: string, untracked: boolean) => call<string>('stash_push', { repo, message, untracked }),
  stashAction: (repo: string, action: string, stash: string) => call<string>('stash_action', { repo, action, stash }),
  conflictVersions: (repo: string, path: string) => call<ConflictVersions>('conflict_versions', { repo, path }),
  resolveConflict: (repo: string, path: string, content: string) => call<string>('resolve_conflict', { repo, path, content }),
  acceptSide: (repo: string, path: string, side: 'ours' | 'theirs') => call<string>('accept_side', { repo, path, side }),
  worktreeList: (repo: string) => call<Worktree[]>('worktree_list', { repo }),
  worktreeAdd: (repo: string, path: string, mode: WorktreeMode, branch: string, start: string) =>
    call<string>('worktree_add', { repo, path, mode, branch, start }),
  worktreeAction: (repo: string, action: 'remove' | 'force-remove' | 'lock' | 'unlock' | 'move' | 'prune', path: string, arg = '') =>
    call<string>('worktree_action', { repo, action, path, arg }),
  openRepo: (path: string) => call<string>('open_repo', { path }),
  summaries: (paths: string[]) => call<RepoSummary[]>('summaries', { paths }),
  scanRepos: (dir: string, depth: number) => call<string[]>('scan_repos', { dir, depth }),
  setGitPath: (path: string) => call<void>('set_git_path', { path }),
  gitVersion: () => call<string>('git_version'),
  openInEditor: (command: string, args: string[], cwd: string) => call<void>('open_in_editor', { command, args, cwd }),
  loadConfig: () => call<unknown>('load_config'),
  saveConfig: (config: unknown) => call<void>('save_config', { config }),
}
