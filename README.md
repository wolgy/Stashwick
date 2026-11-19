# Stashwick

A small, fast desktop Git client modeled on the Git tooling in JetBrains IDEs, for when your editor (Zed, Helix, Neovim…) has only basic Git support.

No account, no subscription, no telemetry. It runs your installed `git`, so your existing config, SSH keys, credential helpers, hooks and signing all work as usual.

Built with Tauri 2 (Rust) and Svelte 5. The installer is about 2–5 MB.

## Features

| | |
|---|---|
| **Workspaces & repositories** | Group repos into workspaces. Switch from the sidebar or with `Ctrl/⌘+1…9`. Each repo shows its branch, ahead/behind, change count and merge/rebase state. *Scan Folder…* adds every repo it finds under a folder. |
| **Commit panel** | Staged / Changes / Unversioned / Conflicts groups. Stage, unstage or roll back whole files, **single hunks, or individual lines** (click the line numbers, Shift-click for a range). Amend, sign-off, *Commit and Push…* |
| **Diff viewer** | Side-by-side or unified. |
| **Log** | Commit graph with lanes, branch/remote/tag labels, filters for branch, text, author and path, and a virtualized list that handles large histories. Details panel shows changed files; click one to see its diff. |
| **Commit context menu** | Checkout revision, new branch or tag here, cherry-pick, revert, reset (soft/mixed/keep/hard), merge into current, rebase current onto. |
| **Branches popup** (`Ctrl/⌘+B`) | Fuzzy search; checkout (remote branches get a local tracking branch); new branch from…; merge; rebase; push; rename; delete. |
| **Push dialog** (`Ctrl/⌘+Shift+K`) | Shows the outgoing commits first. Publishes new branches with upstream. Optional `--force-with-lease`. |
| **Conflict resolver** | 3-way merge: Yours | Result | Theirs. Non-conflicting changes are applied automatically. For each conflict pick left, right, both (either order), base, or edit the text by hand. Handles binary files and modify/delete conflicts. |
| **Merge / rebase / cherry-pick state** | A banner with *Resolve*, *Continue*, *Skip* and *Abort*. |
| **Worktrees** | *Worktrees…* in the toolbar (or the Branches popup, or a repo's right-click menu) lists every worktree with its branch and whether it is locked or missing. From there you can create one (new branch, existing branch, remote branch or detached), open it in Stashwick or your editor, lock or unlock it, move it, remove it, or prune stale entries. A branch checked out in another worktree is marked ⧉. *Checkout* on that branch offers to switch to its worktree, and the right-click menu has *Checkout in New Worktree…*. Linked worktrees are marked ⧉ in the sidebar. |
| **Stash** | Stash (optionally including untracked files), then pop, apply or drop. |
| **Editor integration** | Double-click a file or use *Open in Editor* (`zed` by default; configurable). |
| **Console** | Every git operation and its output, for when something goes wrong. |

The app refreshes automatically when you switch back to it and every few seconds while focused, so edits you make in Zed show up right away.

### Keyboard shortcuts

| | |
|---|---|
| `Ctrl/⌘+K` | Commit panel (focuses the message box) |
| `Ctrl/⌘+Enter` | Commit (in the message box) · `+Shift` = commit and push |
| `Ctrl/⌘+Shift+K` | Push |
| `Ctrl/⌘+T` | Pull (right-click *Pull* to choose merge/rebase) |
| `Ctrl/⌘+B` | Branches popup |
| `Ctrl/⌘+L` | Log |
| `Ctrl/⌘+1…9` | Switch repository |
| `F5` / `Ctrl/⌘+R` | Refresh |
| File list: `↑ ↓` · `Space` (stage/unstage) · `Enter` (open / resolve) | |

## Install

### Option A: let GitHub build it (easiest)

1. Push this folder to a GitHub repository (it can be private).
2. Push a tag (`git tag v0.1.0 && git push --tags`), or open **Actions → Build installers → Run workflow**.
3. Download your installer from the draft **Release** or from the workflow run's artifacts:
   - macOS: `.dmg` (Apple Silicon and Intel)
   - Windows: `.msi` / `-setup.exe`
   - Linux: `.deb` / `.rpm` / `.AppImage`

These builds are not code-signed. On first launch:

- **macOS** says the app "is damaged" or "can't be opened". Run `xattr -cr /Applications/Stashwick.app` once, or right-click → Open.
- **Windows** SmartScreen: click *More info → Run anyway*.

### Option B: build locally

Prerequisites: [Rust](https://rustup.rs), Node.js 20+, and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) (Xcode CLT on macOS, WebView2 + MSVC build tools on Windows, `libwebkit2gtk-4.1-dev` etc. on Linux).

```sh
npm install
npm run app:build        # installers land in src-tauri/target/release/bundle/
npm run app:dev          # run with hot reload while hacking on the UI
npm test                 # backend tests (spin up temporary git repos)
```

## Notes

- **Credentials**: Stashwick never prompts for passwords. Fetch, pull and push over HTTPS use your credential helper (macOS Keychain, Git Credential Manager on Windows, `libsecret`/`store` on Linux). SSH uses your agent. If a push fails with an authentication error, run `git push` once in a terminal so the helper stores the credentials.
- **PATH on macOS**: GUI apps don't inherit your shell's PATH. Stashwick adds `/opt/homebrew/bin`, `/usr/local/bin` and `~/.local/bin` itself. If `git` or `zed` lives somewhere else, set the full path in Settings.
- **Settings and workspaces** are stored in `config.json` in the OS config directory (`~/Library/Application Support/dev.stashwick.app/`, `%APPDATA%\dev.stashwick.app\`, `~/.config/dev.stashwick.app/`).

## Code map

```
src-tauri/src/git.rs     every git operation (status, diff, log, branches, merge, stash, conflicts…) + tests
src-tauri/src/patch.rs   unified-diff parser and partial-patch builder for hunk/line staging
src-tauri/src/lib.rs     Tauri commands, config persistence, editor launch
src/lib/store.svelte.ts  app state, refresh, op() wrapper (busy/console/toasts), workspaces
src/lib/actions.ts       push/pull/branch/reset/… flows with their dialogs
src/lib/graph.ts         commit-graph lane layout
src/lib/diff3.ts         Myers line diff + 3-way merge for the conflict resolver
src/components/          Sidebar, CommitView, DiffView, LogView, BranchesPopup, MergeTool, Panels, Overlays
```

Ideas for later: interactive rebase (reorder, squash, reword), file history and blame view, a shelf, syntax highlighting in diffs, submodule support.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
