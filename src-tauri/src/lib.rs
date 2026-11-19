mod git;
mod patch;

use git::{FileStatus, LineSel, LogQuery};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use tauri::Manager;

type R<T> = Result<T, String>;

async fn bg<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// One lock per repository path: mutating git commands on the same repo run
/// one at a time instead of racing for `index.lock`. Read-only commands
/// (status, diff, log…) don't take it, so polling never waits on a push.
static REPO_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);

fn repo_lock(repo: &str) -> Arc<Mutex<()>> {
    REPO_LOCKS.lock().unwrap_or_else(|e| e.into_inner()).entry(repo.to_string()).or_default().clone()
}

/// Declares an async Tauri command that runs a blocking git function off the main thread.
/// `cmd!` is for read-only commands, `cmd_mut!` for ones that change the repository.
macro_rules! cmd {
    ($name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty => $body:expr) => {
        #[tauri::command]
        async fn $name(repo: String, $($arg: $ty),*) -> R<$ret> {
            bg(move || { let repo = PathBuf::from(repo); let repo: &Path = &repo; let f = $body; f(repo) }).await
        }
    };
}
macro_rules! cmd_mut {
    ($name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty => $body:expr) => {
        #[tauri::command]
        async fn $name(repo: String, $($arg: $ty),*) -> R<$ret> {
            bg(move || {
                let lock = repo_lock(&repo);
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                let repo = PathBuf::from(repo);
                let repo: &Path = &repo;
                let f = $body;
                f(repo)
            })
            .await
        }
    };
}

cmd!(status() -> git::RepoStatus => |r| git::status(r));
cmd!(file_diff(path: String, orig_path: Option<String>, staged: bool, untracked: bool) -> patch::FileDiff
    => |r| git::file_diff(r, &path, orig_path.as_deref(), staged, untracked));
cmd_mut!(apply_lines(path: String, orig_path: Option<String>, mode: String, fingerprint: String, sel: Vec<LineSel>) -> String
    => |r| git::apply_lines(r, &path, orig_path.as_deref(), &mode, &fingerprint, sel));
cmd_mut!(stage(paths: Vec<String>) -> String => |r| git::stage(r, &paths));
cmd_mut!(unstage(paths: Vec<String>) -> String => |r| git::unstage(r, &paths));
cmd_mut!(rollback(files: Vec<FileStatus>) -> String => |r| git::rollback(r, &files));
cmd_mut!(commit(message: String, amend: bool, signoff: bool) -> String => |r| git::commit(r, &message, amend, signoff));
cmd!(last_message() -> String => |r| git::last_message(r));
cmd!(log(query: LogQuery) -> Vec<git::Commit> => |r| git::log(r, &query));
cmd!(commit_detail(hash: String) -> git::CommitDetail => |r| git::commit_detail(r, &hash));
cmd!(commit_file_diff(hash: String, path: String, orig_path: Option<String>) -> patch::FileDiff
    => |r| git::commit_file_diff(r, &hash, &path, orig_path.as_deref()));
cmd!(branches() -> Vec<git::BranchInfo> => |r| git::branches(r));
cmd!(remotes() -> Vec<String> => |r| git::remotes(r));
cmd_mut!(checkout(name: String, kind: String) -> String => |r| git::checkout(r, &name, &kind));
cmd_mut!(create_branch(name: String, start: String, checkout: bool) -> String => |r| git::create_branch(r, &name, &start, checkout));
cmd_mut!(delete_branch(name: String, kind: String, force: bool) -> String => |r| git::delete_branch(r, &name, &kind, force));
cmd_mut!(rename_branch(old: String, new: String) -> String => |r| git::rename_branch(r, &old, &new));
cmd_mut!(merge(rev: String) -> String => |r| git::merge(r, &rev));
cmd_mut!(rebase(onto: String) -> String => |r| git::rebase(r, &onto));
cmd_mut!(sequencer(op: String, action: String) -> String => |r| git::sequencer(r, &op, &action));
cmd_mut!(cherry_pick(hash: String) -> String => |r| git::cherry_pick(r, &hash));
cmd_mut!(revert(hash: String) -> String => |r| git::revert(r, &hash));
cmd_mut!(reset(hash: String, mode: String) -> String => |r| git::reset(r, &hash, &mode));
cmd_mut!(squash(hashes: Vec<String>, message: String) -> String => |r| git::squash(r, &hashes, &message));
cmd_mut!(create_tag(name: String, rev: String, message: String) -> String => |r| git::create_tag(r, &name, &rev, &message));
cmd_mut!(fetch() -> String => |r| git::fetch(r));
cmd_mut!(pull(rebase: bool) -> String => |r| git::pull(r, rebase));
cmd_mut!(push(branch: Option<String>, force: bool, tags: bool) -> String => |r| git::push(r, branch.as_deref(), force, tags));
cmd!(stash_list() -> Vec<git::StashEntry> => |r| git::stash_list(r));
cmd_mut!(stash_push(message: String, untracked: bool) -> String => |r| git::stash_push(r, &message, untracked));
cmd_mut!(stash_action(action: String, stash: String) -> String => |r| git::stash_action(r, &action, &stash));
cmd!(conflict_versions(path: String) -> git::ConflictVersions => |r| git::conflict_versions(r, &path));
cmd_mut!(resolve_conflict(path: String, content: String) -> String => |r| git::resolve_conflict(r, &path, &content));
cmd!(worktree_list() -> Vec<git::Worktree> => |r| git::worktree_list(r));
cmd_mut!(worktree_add(path: String, mode: String, branch: String, start: String) -> String
    => |r| git::worktree_add(r, &path, &mode, &branch, &start));
cmd_mut!(worktree_action(action: String, path: String, arg: String) -> String => |r| git::worktree_action(r, &action, &path, &arg));
cmd_mut!(accept_side(path: String, side: String) -> String => |r| git::accept_side(r, &path, &side));

#[tauri::command]
async fn open_repo(path: String) -> R<String> {
    bg(move || git::toplevel(Path::new(&path))).await
}

#[tauri::command]
async fn summaries(paths: Vec<String>) -> R<Vec<git::RepoSummary>> {
    // A few workers rather than a thread per repo: each summary spawns several
    // git processes, and big workspaces would otherwise hit the disk all at once.
    bg(move || {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let next = AtomicUsize::new(0);
        let out = Mutex::new(Vec::with_capacity(paths.len()));
        std::thread::scope(|s| {
            for _ in 0..paths.len().min(4) {
                s.spawn(|| {
                    while let Some(p) = paths.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let r = git::summary(Path::new(p));
                        out.lock().unwrap_or_else(|e| e.into_inner()).push(r);
                    }
                });
            }
        });
        Ok(out.into_inner().unwrap_or_else(|e| e.into_inner()))
    })
    .await
}

#[tauri::command]
async fn scan_repos(dir: String, depth: usize) -> R<Vec<String>> {
    bg(move || Ok(git::scan_repos(Path::new(&dir), depth))).await
}

#[tauri::command]
fn set_git_path(path: String) {
    git::set_git_exe(&path);
}

#[tauri::command]
async fn git_version() -> R<String> {
    bg(|| git::run_s(&std::env::temp_dir(), &["--version"]).map(|s| s.trim().to_string())).await
}

/// Launch an editor command (e.g. `zed`) with the given file/dir arguments.
#[tauri::command]
fn open_in_editor(command: String, args: Vec<String>, cwd: String) -> R<()> {
    let mut parts = command.split_whitespace();
    let prog = parts.next().ok_or("No editor command configured")?;
    let mut c = std::process::Command::new(prog);
    // Absolute paths, so a repo file named e.g. `--some-flag` can't become an editor option.
    let base = Path::new(&cwd);
    c.args(parts).args(args.iter().map(|a| base.join(a))).current_dir(&cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("Could not launch '{prog}': {e}. Set the editor command in Settings."))
}

fn config_path(app: &tauri::AppHandle) -> R<PathBuf> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("config.json"))
}

#[tauri::command]
fn load_config(app: tauri::AppHandle) -> R<serde_json::Value> {
    let p = config_path(&app)?;
    let Ok(s) = std::fs::read_to_string(&p) else { return Ok(serde_json::Value::Null) };
    serde_json::from_str(&s).map_err(|e| {
        // Move the unreadable file aside so the next save (with defaults)
        // doesn't silently destroy the user's workspaces.
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let bak = p.with_extension(format!("json.broken-{secs}"));
        match std::fs::rename(&p, &bak) {
            Ok(()) => format!("{e}. The file was moved to {}; starting with default settings.", bak.display()),
            Err(re) => format!("{e} (could not back it up: {re})"),
        }
    })
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: serde_json::Value) -> R<()> {
    let p = config_path(&app)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// GUI apps on macOS/Linux don't inherit the shell PATH, so git/zed from
/// Homebrew or ~/.local/bin would not be found. Append the usual locations.
fn extend_path() {
    #[cfg(not(windows))]
    {
        let mut path = std::env::var("PATH").unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_default();
        for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin", &format!("{home}/.local/bin"), &format!("{home}/bin")] {
            if !path.split(':').any(|p| p == extra) {
                if !path.is_empty() {
                    path.push(':');
                }
                path.push_str(extra);
            }
        }
        // SAFETY: called once at startup before any threads are spawned.
        unsafe { std::env::set_var("PATH", path) };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    extend_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            status, file_diff, apply_lines, stage, unstage, rollback, commit, last_message, log, commit_detail,
            commit_file_diff, branches, remotes, checkout, create_branch, delete_branch, rename_branch, merge, rebase,
            sequencer, cherry_pick, revert, reset, squash, create_tag, fetch, pull, push, stash_list, stash_push, stash_action,
            conflict_versions, resolve_conflict, accept_side, worktree_list, worktree_add, worktree_action, open_repo, summaries, scan_repos, set_git_path,
            git_version, open_in_editor, load_config, save_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running Stashwick");
}
