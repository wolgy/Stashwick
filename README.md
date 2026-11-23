# Stashwick

A small, fast desktop Git client modeled on the Git tooling in JetBrains IDEs, for when your editor (Zed, Helix, Neovim…) has only basic Git support.

No account, no subscription, no telemetry. It runs your installed `git`, so your existing config, SSH keys, credential helpers, hooks and signing all work as usual.

Built with Tauri 2 (Rust) and Svelte 5. The installer is about 2–5 MB.

## Features

| | |
|---|---|
| **Workspaces & repositories** | Group repos into workspaces. Switch from the sidebar or with `Ctrl/⌘+1…9`. Each repo shows its branch, ahead/behind, change count and merge/rebase state. *Scan Folder…* adds every repo it finds under a folder. |
| **Commit panel** | Staged / Changes / Unversioned / Conflicts groups. *🗀 Folders* shows the files as a directory tree (also in the Log's changed files); right-click or hover a folder to stage, unstage or roll back everything in it. Stage, unstage or roll back whole files, **single hunks, or individual lines** (click the line numbers, Shift-click for a range). Amend, sign-off, *Commit and Push…* |
| **Diff viewer** | Side-by-side or unified. |
| **Log** | Commit graph with lanes (commits already in `main`/`master`, locally or on a remote, are grey; unmerged work is coloured), branch/remote/tag labels, filters for branch, text, author and path, and a virtualized list that handles large histories. Details panel shows changed files; click one to see its diff. |
| **Commit context menu** | Checkout revision, new branch or tag here, cherry-pick, revert, reset (soft/mixed/keep/hard), merge into current, rebase current onto. |
| **Squash commits** | Select several commits in the Log (`Ctrl/⌘`-click, `Shift`-click), right-click → *Squash N Commits…*, and edit the combined message. They don't have to be adjacent: later ones move up to the oldest, and the commits in between are replayed after it. Uncommitted changes are stashed and restored automatically. If a conflict stops it, use the rebase banner as usual. |
| **Branches popup** (`Ctrl/⌘+B`) | Fuzzy search; checkout (remote branches get a local tracking branch); new branch from…; merge; rebase; push; rename; delete. |
| **Push dialog** (`Ctrl/⌘+Shift+K`) | Shows the outgoing commits first. Publishes new branches with upstream. Optional `--force-with-lease`. |
| **Conflict resolver** | 3-way merge: Yours | Result | Theirs. Non-conflicting changes are applied automatically. For each conflict pick left, right, both (either order), base, or edit the text by hand. Handles binary files and modify/delete conflicts. |
| **Merge / rebase / cherry-pick state** | A banner with *Resolve*, *Continue*, *Skip* and *Abort*. |
| **Worktrees** | The ⧉ button before the branch in the toolbar shows the current worktree. Clicking it (or the Branches popup, or a repo's right-click menu) lists every worktree with its branch and whether it is locked or missing. From there you can create one (new branch, existing branch, remote branch or detached), open it in Stashwick or your editor, lock or unlock it, move it, remove it, or prune stale entries. A branch checked out in another worktree is marked ⧉. *Checkout* on that branch offers to switch to its worktree, and the right-click menu has *Checkout in New Worktree…*. Linked worktrees are marked ⧉ in the sidebar. |
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

Download the installer for your platform from the [Releases](https://github.com/wolgy/Stashwick/releases) page:

- **macOS**: `.dmg` (Apple Silicon and Intel)
- **Windows**: `.msi` or `-setup.exe`
- **Linux**: `.deb`, `.rpm` or `.AppImage`

Stashwick needs `git` installed and on your PATH.

The builds are not code-signed yet, so on first launch:

- **macOS** may say the app "is damaged" or "can't be opened". Right-click the app → *Open*, or run `xattr -cr /Applications/Stashwick.app` once.
- **Windows** SmartScreen: click *More info → Run anyway*.

To build it yourself, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Notes

- **Credentials**: Stashwick never prompts for passwords. Fetch, pull and push over HTTPS use your credential helper (macOS Keychain, Git Credential Manager on Windows, `libsecret`/`store` on Linux). SSH uses your agent. If a push fails with an authentication error, run `git push` once in a terminal so the helper stores the credentials.
- **PATH on macOS**: GUI apps don't inherit your shell's PATH. Stashwick adds `/opt/homebrew/bin`, `/usr/local/bin` and `~/.local/bin` itself. If `git` or `zed` lives somewhere else, set the full path in Settings.
- **Settings and workspaces** are stored in `config.json` in the OS config directory (`~/Library/Application Support/dev.stashwick.app/`, `%APPDATA%\dev.stashwick.app\`, `~/.config/dev.stashwick.app/`).

## Roadmap

Ideas for later: more interactive rebase (reorder, reword, drop), file history and blame view, a shelf, syntax highlighting in diffs, submodule support.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
