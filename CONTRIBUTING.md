# Contributing to Stashwick

Bug reports, ideas and pull requests are welcome. For larger changes, please open an issue first so we can agree on the approach.

By contributing you agree that your contributions are licensed under the project's license, GPL-3.0-only.

## Build from source

Prerequisites: [Rust](https://rustup.rs), Node.js 20+, and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) (Xcode CLT on macOS, WebView2 + MSVC build tools on Windows, `libwebkit2gtk-4.1-dev` etc. on Linux).

```sh
npm install
npm run app:dev          # run with hot reload while working on the UI
npm run app:build        # installers land in src-tauri/target/release/bundle/
```

## Checks

Run these before opening a pull request:

```sh
npm run check            # svelte-check + TypeScript
npm test                 # backend tests (they create temporary git repos)
```

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

## Releasing (maintainers)

1. Set the new version in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, and commit.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
3. The **Build installers** workflow (`.github/workflows/build.yml`) builds macOS, Windows and Linux installers and attaches them to a draft GitHub Release.
4. Check the draft, write the release notes, and publish it.
