//! Thin layer over the user's installed `git` executable.
//! Every function is synchronous; lib.rs runs them on a blocking thread pool.

use crate::patch::{self, FileDiff};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;

pub type R<T> = Result<T, String>;

static GIT_EXE: RwLock<String> = RwLock::new(String::new());
const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;

pub fn set_git_exe(path: &str) {
    *GIT_EXE.write().unwrap() = path.trim().to_string();
}

fn exe() -> String {
    let g = GIT_EXE.read().unwrap();
    if g.is_empty() { "git".into() } else { g.clone() }
}

pub struct Out {
    pub ok: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

pub fn run_full(repo: &Path, args: &[&str], input: Option<Vec<u8>>) -> R<Out> {
    let mut cmd = Command::new(exe());
    cmd.arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=never", "-c", "log.showSignature=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .env("GIT_PAGER", "cat")
        .env("LANGUAGE", "C")
        .env("LC_MESSAGES", "C")
        .env_remove("LC_ALL") // would override LC_MESSAGES; we parse some git messages
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not run git ({}): {e}. Is git installed / is the path in Settings correct?", exe()))?;
    let writer = if let Some(data) = input {
        let mut stdin = child.stdin.take().unwrap();
        Some(std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        }))
    } else {
        None
    };
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if let Some(w) = writer {
        let _ = w.join();
    }
    Ok(Out {
        ok: out.status.success(),
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

fn err_of(o: &Out) -> String {
    let e = o.stderr.trim();
    if !e.is_empty() {
        return e.to_string();
    }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s.is_empty() { "git command failed".into() } else { s }
}

pub fn run(repo: &Path, args: &[&str]) -> R<Vec<u8>> {
    let o = run_full(repo, args, None)?;
    if o.ok { Ok(o.stdout) } else { Err(err_of(&o)) }
}

pub fn run_s(repo: &Path, args: &[&str]) -> R<String> {
    run(repo, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Run and return combined stdout+stderr (for user-facing output of mutating ops).
pub fn run_op(repo: &Path, args: &[&str]) -> R<String> {
    run_op_in(repo, args, None)
}

fn run_op_in(repo: &Path, args: &[&str], input: Option<Vec<u8>>) -> R<String> {
    let o = run_full(repo, args, input)?;
    let mut s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let e = o.stderr.trim();
    if !e.is_empty() {
        if !s.is_empty() {
            s.push('\n');
        }
        s.push_str(e);
    }
    if o.ok { Ok(s) } else { Err(if s.is_empty() { "git command failed".into() } else { s }) }
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Resolve a repo-relative path, refusing anything that would escape the
/// working tree (absolute paths, `..`, drive prefixes). `Path::join` silently
/// discards the base for absolute input, so every filesystem access to a
/// caller-supplied path must go through here.
pub fn in_repo(repo: &Path, rel: &str) -> R<PathBuf> {
    use std::path::Component;
    let p = Path::new(rel);
    if rel.is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
        return Err(format!("{rel}: path is outside the repository"));
    }
    Ok(repo.join(p))
}

/// Refuse user-supplied names/revisions that git would parse as an option.
fn not_option<'a>(what: &str, v: &'a str) -> R<&'a str> {
    if v.starts_with('-') { Err(format!("Invalid {what} \"{v}\": must not start with '-'")) } else { Ok(v) }
}

// ───────────────────────────── repository ─────────────────────────────

pub fn toplevel(path: &Path) -> R<String> {
    let s = run_s(path, &["rev-parse", "--show-toplevel"]).map_err(|_| format!("{} is not inside a git repository", path.display()))?;
    Ok(s.trim().to_string())
}

pub fn git_dir(repo: &Path) -> R<PathBuf> {
    Ok(PathBuf::from(run_s(repo, &["rev-parse", "--absolute-git-dir"])?.trim()))
}

fn has_head(repo: &Path) -> bool {
    run(repo, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok()
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    /// "", "merging", "rebasing", "cherry-picking", "reverting"
    pub kind: String,
    pub detail: Option<String>,
    pub ours_label: String,
    pub theirs_label: String,
}

pub fn repo_state(repo: &Path) -> R<RepoState> {
    let gd = git_dir(repo)?;
    let read = |p: &str| std::fs::read_to_string(gd.join(p)).ok().map(|s| s.trim().to_string());
    let short = |s: String| s.trim_start_matches("refs/heads/").to_string();
    let mut st = RepoState {
        ours_label: "Yours (HEAD)".into(),
        theirs_label: "Theirs".into(),
        ..Default::default()
    };
    for dir in ["rebase-merge", "rebase-apply"] {
        if gd.join(dir).is_dir() && (dir == "rebase-merge" || !gd.join(dir).join("applying").exists()) {
            st.kind = "rebasing".into();
            let head = read(&format!("{dir}/head-name")).map(short).unwrap_or_else(|| "detached".into());
            let onto = read(&format!("{dir}/onto")).map(|s| s.chars().take(8).collect::<String>()).unwrap_or_default();
            let (n, e) = if dir == "rebase-merge" {
                (read("rebase-merge/msgnum"), read("rebase-merge/end"))
            } else {
                (read("rebase-apply/next"), read("rebase-apply/last"))
            };
            let step = match (n, e) {
                (Some(n), Some(e)) => format!(" (step {n}/{e})"),
                _ => String::new(),
            };
            st.detail = Some(format!("Rebasing {head} onto {onto}{step}"));
            st.ours_label = "Upstream (rebasing onto)".into();
            st.theirs_label = format!("Yours ({head})");
            return Ok(st);
        }
    }
    if gd.join("MERGE_HEAD").exists() {
        st.kind = "merging".into();
        let msg = read("MERGE_MSG").and_then(|m| m.lines().next().map(|s| s.to_string()));
        st.detail = msg.clone();
        let theirs = msg
            .as_deref()
            .and_then(|m| m.split('\'').nth(1))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "merged branch".into());
        st.theirs_label = format!("Theirs ({theirs})");
    } else if gd.join("CHERRY_PICK_HEAD").exists() {
        st.kind = "cherry-picking".into();
        let h = read("CHERRY_PICK_HEAD").unwrap_or_default();
        st.detail = Some(format!("Cherry-picking {}", &h[..h.len().min(8)]));
        st.theirs_label = "Cherry-picked commit".into();
    } else if gd.join("REVERT_HEAD").exists() {
        st.kind = "reverting".into();
        let h = read("REVERT_HEAD").unwrap_or_default();
        st.detail = Some(format!("Reverting {}", &h[..h.len().min(8)]));
        st.theirs_label = "Reverted commit (inverse)".into();
    }
    Ok(st)
}

// ───────────────────────────── status ─────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub orig_path: Option<String>,
    pub index: char,
    pub worktree: char,
    pub conflict: bool,
    pub untracked: bool,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub files: Vec<FileStatus>,
    pub state: RepoState,
}

fn xy(c: char) -> char {
    if c == '.' { ' ' } else { c }
}

fn parse_status(raw: &[u8]) -> RepoStatus {
    let mut st = RepoStatus::default();
    let recs: Vec<String> = raw.split(|b| *b == 0).map(lossy).collect();
    let mut i = 0;
    while i < recs.len() {
        let r = &recs[i];
        i += 1;
        if r.is_empty() {
            continue;
        }
        if let Some(h) = r.strip_prefix("# branch.head ") {
            st.branch = if h == "(detached)" { None } else { Some(h.to_string()) };
        } else if let Some(h) = r.strip_prefix("# branch.oid ") {
            st.head = if h == "(initial)" { None } else { Some(h.to_string()) };
        } else if let Some(u) = r.strip_prefix("# branch.upstream ") {
            st.upstream = Some(u.to_string());
        } else if let Some(ab) = r.strip_prefix("# branch.ab ") {
            for p in ab.split(' ') {
                if let Some(a) = p.strip_prefix('+') {
                    st.ahead = a.parse().unwrap_or(0);
                } else if let Some(b) = p.strip_prefix('-') {
                    st.behind = b.parse().unwrap_or(0);
                }
            }
        } else if r.starts_with("1 ") {
            let f: Vec<&str> = r.splitn(9, ' ').collect();
            if f.len() == 9 {
                let c: Vec<char> = f[1].chars().collect();
                st.files.push(FileStatus {
                    path: f[8].into(),
                    orig_path: None,
                    index: xy(c[0]),
                    worktree: xy(c[1]),
                    conflict: false,
                    untracked: false,
                });
            }
        } else if r.starts_with("2 ") {
            let f: Vec<&str> = r.splitn(10, ' ').collect();
            let orig = recs.get(i).cloned();
            i += 1;
            if f.len() == 10 {
                let c: Vec<char> = f[1].chars().collect();
                st.files.push(FileStatus {
                    path: f[9].into(),
                    orig_path: orig,
                    index: xy(c[0]),
                    worktree: xy(c[1]),
                    conflict: false,
                    untracked: false,
                });
            }
        } else if r.starts_with("u ") {
            let f: Vec<&str> = r.splitn(11, ' ').collect();
            if f.len() == 11 {
                let c: Vec<char> = f[1].chars().collect();
                st.files.push(FileStatus {
                    path: f[10].into(),
                    orig_path: None,
                    index: c[0],
                    worktree: c[1],
                    conflict: true,
                    untracked: false,
                });
            }
        } else if let Some(p) = r.strip_prefix("? ") {
            st.files.push(FileStatus {
                path: p.into(),
                orig_path: None,
                index: '?',
                worktree: '?',
                conflict: false,
                untracked: true,
            });
        }
    }
    st
}

/// Background polling runs without any user action, so it must not execute
/// commands configured by the repository itself (`core.fsmonitor=<cmd>` in a
/// cloned/downloaded repo's .git/config would otherwise run on open).
const POLL_SAFE: [&str; 2] = ["-c", "core.fsmonitor=false"];

pub fn status(repo: &Path) -> R<RepoStatus> {
    let raw = run(repo, &[POLL_SAFE[0], POLL_SAFE[1], "--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"])?;
    let mut st = parse_status(&raw);
    st.root = repo.to_string_lossy().into_owned();
    st.state = repo_state(repo)?;
    Ok(st)
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub path: String,
    pub branch: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub changes: usize,
    pub conflicts: usize,
    pub state: String,
    pub error: Option<String>,
    /// true when this checkout is a linked worktree (not the main working tree)
    pub linked: bool,
}

pub fn summary(repo: &Path) -> RepoSummary {
    let mut s = RepoSummary { path: repo.to_string_lossy().into_owned(), ..Default::default() };
    match run(repo, &[POLL_SAFE[0], POLL_SAFE[1], "--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch", "--untracked-files=normal"]) {
        Ok(raw) => {
            let st = parse_status(&raw);
            s.branch = st.branch.or(st.head.map(|h| format!("({})", &h[..8.min(h.len())])));
            s.ahead = st.ahead;
            s.behind = st.behind;
            s.changes = st.files.len();
            s.conflicts = st.files.iter().filter(|f| f.conflict).count();
            s.state = repo_state(repo).map(|r| r.kind).unwrap_or_default();
            s.linked = is_linked_worktree(repo);
        }
        Err(e) => s.error = Some(e),
    }
    s
}

// ───────────────────────────── diffs ─────────────────────────────

const DIFF_OPTS: [&str; 6] = ["--no-color", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/", "-U3", "--no-renames"];

fn worktree_raw_diff(repo: &Path, path: &str, orig: Option<&str>, staged: bool) -> R<(Vec<u8>, patch::RawDiff)> {
    let mut args: Vec<&str> = vec!["diff"];
    args.extend_from_slice(&DIFF_OPTS);
    if staged {
        args.pop(); // drop --no-renames so staged renames show as renames
        args.push("-M");
        args.push("--cached");
    }
    args.push("--");
    if let (true, Some(o)) = (staged, orig) {
        args.push(o);
    }
    args.push(path);
    let bytes = run(repo, &args)?;
    let raw = if bytes.len() > MAX_DIFF_BYTES {
        patch::RawDiff { fingerprint: patch::fingerprint(&bytes), ..Default::default() }
    } else {
        patch::parse(&bytes)
    };
    Ok((bytes, raw))
}

pub fn file_diff(repo: &Path, path: &str, orig: Option<&str>, staged: bool, untracked: bool) -> R<FileDiff> {
    if untracked {
        let full = in_repo(repo, path)?;
        let meta = std::fs::metadata(&full).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            return Ok(FileDiff { untracked: true, note: Some("Directory (nested repository or submodule)".into()), ..Default::default() });
        }
        if meta.len() as usize > MAX_DIFF_BYTES {
            return Ok(FileDiff { untracked: true, too_large: true, ..Default::default() });
        }
        let content = std::fs::read(&full).map_err(|e| e.to_string())?;
        return Ok(patch::untracked_view(&content));
    }
    let (bytes, raw) = worktree_raw_diff(repo, path, orig, staged)?;
    if bytes.len() > MAX_DIFF_BYTES {
        return Ok(FileDiff { too_large: true, fingerprint: raw.fingerprint, ..Default::default() });
    }
    Ok(raw.to_view())
}

#[derive(Deserialize, Debug)]
pub struct LineSel {
    pub hunk: usize,
    pub line: usize,
}

/// mode: "stage" (worktree diff → index), "unstage" (cached diff, reversed),
/// "discard" (worktree diff, reversed into worktree)
pub fn apply_lines(repo: &Path, path: &str, orig: Option<&str>, mode: &str, fingerprint: &str, sel: Vec<LineSel>) -> R<String> {
    let staged = mode == "unstage";
    let (_, raw) = worktree_raw_diff(repo, path, orig, staged)?;
    if raw.fingerprint != fingerprint {
        return Err("The file changed since the diff was shown. Refreshed — please select again.".into());
    }
    let set: HashSet<(usize, usize)> = sel.into_iter().map(|s| (s.hunk, s.line)).collect();
    let reverse = mode != "stage";
    let p = raw.build_partial(&set, reverse).ok_or("Nothing selected")?;
    let mut args = vec!["apply", "--whitespace=nowarn", "--recount"];
    if mode != "discard" {
        args.push("--cached");
    }
    if reverse {
        args.push("-R");
    }
    args.push("-");
    run_op_in(repo, &args, Some(p))
}

pub fn commit_file_diff(repo: &Path, hash: &str, path: &str, orig: Option<&str>) -> R<FileDiff> {
    let parents = commit_parents(repo, hash)?;
    let mut args: Vec<String> = Vec::new();
    if let Some(p) = parents.first() {
        args.push("diff".into());
        args.extend(DIFF_OPTS.iter().take(5).map(|s| s.to_string()));
        args.push("-M".into());
        args.push(p.clone());
        args.push(hash.into());
    } else {
        args.extend(["diff-tree", "--root", "-p", "--no-commit-id", "-M", "--no-color", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/"].map(String::from));
        args.push(hash.into());
    }
    args.push("--".into());
    if let Some(o) = orig {
        args.push(o.into());
    }
    args.push(path.into());
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let bytes = run(repo, &a)?;
    if bytes.len() > MAX_DIFF_BYTES {
        return Ok(FileDiff { too_large: true, ..Default::default() });
    }
    Ok(patch::parse(&bytes).to_view())
}

// ───────────────────────────── index ops ─────────────────────────────

pub fn stage(repo: &Path, paths: &[String]) -> R<String> {
    let mut a = vec!["add", "-A", "--"];
    a.extend(paths.iter().map(|s| s.as_str()));
    run_op(repo, &a)
}

pub fn unstage(repo: &Path, paths: &[String]) -> R<String> {
    let mut a: Vec<&str> = if has_head(repo) { vec!["reset", "-q", "HEAD", "--"] } else { vec!["rm", "--cached", "-r", "-q", "--"] };
    a.extend(paths.iter().map(|s| s.as_str()));
    run_op(repo, &a)
}

/// IDEA-style "Rollback": bring files back to HEAD (index + worktree).
pub fn rollback(repo: &Path, files: &[FileStatus]) -> R<String> {
    // Validate everything first so a bad entry can't leave a half-done rollback.
    let mut delete: Vec<PathBuf> = Vec::new();
    for f in files.iter().filter(|f| f.untracked) {
        let p = in_repo(repo, &f.path)?;
        if p.is_dir() {
            return Err(format!("{}: is a directory (nested repository?) — remove it manually", f.path));
        }
        delete.push(p);
    }
    let mut restore: Vec<&str> = Vec::new();
    let mut remove_idx: Vec<&str> = Vec::new();
    for f in files {
        if f.untracked {
            continue;
        } else if f.index == 'A' {
            remove_idx.push(&f.path);
        } else if f.index == 'R' || f.index == 'C' {
            remove_idx.push(&f.path);
            if let Some(o) = &f.orig_path {
                restore.push(o);
            }
        } else {
            restore.push(&f.path);
        }
    }
    for p in &delete {
        std::fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let mut out = String::new();
    if !remove_idx.is_empty() {
        let mut a = vec!["rm", "-f", "-q", "--"];
        a.extend(remove_idx);
        out.push_str(&run_op(repo, &a)?);
    }
    if !restore.is_empty() {
        let mut a = vec!["restore", "--source=HEAD", "--staged", "--worktree", "--"];
        a.extend(restore);
        out.push_str(&run_op(repo, &a)?);
    }
    Ok(out)
}

pub fn commit(repo: &Path, message: &str, amend: bool, signoff: bool) -> R<String> {
    let mut a = vec!["commit"];
    if amend {
        a.push("--amend");
    }
    if signoff {
        a.push("--signoff");
    }
    if amend && message.trim().is_empty() {
        a.push("--no-edit");
        return run_op(repo, &a);
    }
    // "whitespace", not "strip": with no editor involved, strip would delete
    // every line starting with '#' (e.g. "#123 Fix crash").
    a.extend(["--cleanup=whitespace", "-F", "-"]);
    run_op_in(repo, &a, Some(message.as_bytes().to_vec()))
}

pub fn last_message(repo: &Path) -> R<String> {
    Ok(run_s(repo, &["log", "-1", "--format=%B"])?.trim_end().to_string())
}

// ───────────────────────────── log ─────────────────────────────

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: String, // head | local | remote | tag
    pub current: bool,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    pub refs: Vec<RefLabel>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub branch: Option<String>,
    pub author: Option<String>,
    pub text: Option<String>,
    pub path: Option<String>,
    pub skip: usize,
    pub limit: usize,
}

fn parse_refs(d: &str) -> Vec<RefLabel> {
    let mut out = Vec::new();
    for item in d.split(", ").filter(|s| !s.is_empty()) {
        if item == "HEAD" {
            out.push(RefLabel { name: "HEAD".into(), kind: "head".into(), current: true });
        } else if let Some(r) = item.strip_prefix("HEAD -> ") {
            out.push(RefLabel { name: r.trim_start_matches("refs/heads/").into(), kind: "local".into(), current: true });
        } else if let Some(r) = item.strip_prefix("tag: ") {
            out.push(RefLabel { name: r.trim_start_matches("refs/tags/").into(), kind: "tag".into(), current: false });
        } else if let Some(r) = item.strip_prefix("refs/heads/") {
            out.push(RefLabel { name: r.into(), kind: "local".into(), current: false });
        } else if let Some(r) = item.strip_prefix("refs/remotes/") {
            if !r.ends_with("/HEAD") {
                out.push(RefLabel { name: r.into(), kind: "remote".into(), current: false });
            }
        }
    }
    out
}

pub fn log(repo: &Path, q: &LogQuery) -> R<Vec<Commit>> {
    let limit = if q.limit == 0 { 1000 } else { q.limit };
    let mut a: Vec<String> = vec![
        "log".into(),
        "--topo-order".into(),
        "--decorate=full".into(),
        "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e".into(),
        format!("--max-count={limit}"),
        format!("--skip={}", q.skip),
    ];
    if let Some(au) = q.author.as_ref().filter(|s| !s.trim().is_empty()) {
        a.push(format!("--author={}", au.trim()));
        a.push("--regexp-ignore-case".into());
    }
    if let Some(t) = q.text.as_ref().filter(|s| !s.trim().is_empty()) {
        a.push(format!("--grep={}", t.trim()));
        a.push("--regexp-ignore-case".into());
        a.push("--fixed-strings".into());
    }
    match q.branch.as_ref().filter(|s| !s.is_empty()) {
        Some(b) => a.push(b.clone()),
        None => {
            if !has_head(repo) && run_s(repo, &["for-each-ref", "--count=1", "refs/heads", "refs/remotes"])?.trim().is_empty() {
                return Ok(vec![]);
            }
            a.extend(["--branches", "--remotes", "--tags"].map(String::from));
            if has_head(repo) {
                a.push("HEAD".into());
            }
        }
    }
    a.push("--".into());
    if let Some(p) = q.path.as_ref().filter(|s| !s.trim().is_empty()) {
        a.push(p.trim().into());
    }
    let args: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
    let raw = run_s(repo, &args)?;
    let mut out = Vec::new();
    for rec in raw.split('\x1e') {
        let rec = rec.trim_start_matches('\n');
        if rec.is_empty() {
            continue;
        }
        let f: Vec<&str> = rec.splitn(7, '\x1f').collect();
        if f.len() < 7 {
            continue;
        }
        out.push(Commit {
            hash: f[0].into(),
            parents: f[1].split(' ').filter(|s| !s.is_empty()).map(String::from).collect(),
            author: f[2].into(),
            email: f[3].into(),
            time: f[4].parse().unwrap_or(0),
            refs: parse_refs(f[5]),
            subject: f[6].into(),
        });
    }
    Ok(out)
}

fn commit_parents(repo: &Path, hash: &str) -> R<Vec<String>> {
    let s = run_s(repo, &["rev-list", "--parents", "-n", "1", hash])?;
    Ok(s.split_whitespace().skip(1).map(String::from).collect())
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub status: String,
    pub path: String,
    pub orig_path: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub author_time: i64,
    pub committer: String,
    pub committer_email: String,
    pub commit_time: i64,
    pub message: String,
    pub files: Vec<ChangedFile>,
}

fn parse_name_status(raw: &[u8]) -> Vec<ChangedFile> {
    let recs: Vec<String> = raw.split(|b| *b == 0).map(lossy).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < recs.len() {
        let s = recs[i].trim().to_string();
        i += 1;
        if s.is_empty() {
            continue;
        }
        let code = s.chars().next().unwrap();
        if code == 'R' || code == 'C' {
            let (o, n) = (recs.get(i).cloned().unwrap_or_default(), recs.get(i + 1).cloned().unwrap_or_default());
            i += 2;
            out.push(ChangedFile { status: code.to_string(), path: n, orig_path: Some(o) });
        } else {
            let p = recs.get(i).cloned().unwrap_or_default();
            i += 1;
            out.push(ChangedFile { status: code.to_string(), path: p, orig_path: None });
        }
    }
    out
}

pub fn commit_detail(repo: &Path, hash: &str) -> R<CommitDetail> {
    let raw = run_s(repo, &["show", "-s", "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%B", hash])?;
    let f: Vec<&str> = raw.splitn(9, '\x1f').collect();
    if f.len() < 9 {
        return Err("Could not read commit".into());
    }
    let parents: Vec<String> = f[1].split(' ').filter(|s| !s.is_empty()).map(String::from).collect();
    let files_raw = match parents.first() {
        Some(p) => run(repo, &["diff", "--no-ext-diff", "-z", "--name-status", "-M", p, hash])?,
        None => run(repo, &["diff-tree", "--root", "--no-commit-id", "-r", "-z", "--name-status", "-M", hash])?,
    };
    Ok(CommitDetail {
        hash: f[0].into(),
        parents,
        author: f[2].into(),
        email: f[3].into(),
        author_time: f[4].parse().unwrap_or(0),
        committer: f[5].into(),
        committer_email: f[6].into(),
        commit_time: f[7].parse().unwrap_or(0),
        message: f[8].trim_end().into(),
        files: parse_name_status(&files_raw),
    })
}

// ───────────────────────────── branches ─────────────────────────────

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub full: String,
    pub kind: String, // local | remote | tag
    pub hash: String,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub gone: bool,
    pub current: bool,
    pub time: i64,
    pub subject: String,
    /// Path of the worktree that has this branch checked out (local branches only).
    pub worktree: Option<String>,
}

pub fn branches(repo: &Path) -> R<Vec<BranchInfo>> {
    let fmt = "--format=%(refname)%1f%(if)%(*objectname)%(then)%(*objectname)%(else)%(objectname)%(end)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(HEAD)%1f%(creatordate:unix)%1f%(worktreepath)%1f%(contents:subject)";
    let raw = run_s(repo, &["for-each-ref", "--sort=-creatordate", fmt, "refs/heads", "refs/remotes", "refs/tags"])?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let f: Vec<&str> = line.splitn(8, '\x1f').collect();
        if f.len() < 8 {
            continue;
        }
        let full = f[0];
        let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
            ("local", n)
        } else if let Some(n) = full.strip_prefix("refs/remotes/") {
            if n.ends_with("/HEAD") {
                continue;
            }
            ("remote", n)
        } else if let Some(n) = full.strip_prefix("refs/tags/") {
            ("tag", n)
        } else {
            continue;
        };
        let (mut ahead, mut behind) = (0, 0);
        for p in f[3].split(", ") {
            if let Some(a) = p.strip_prefix("ahead ") {
                ahead = a.parse().unwrap_or(0);
            } else if let Some(b) = p.strip_prefix("behind ") {
                behind = b.parse().unwrap_or(0);
            }
        }
        out.push(BranchInfo {
            name: name.into(),
            full: full.into(),
            kind: kind.into(),
            hash: f[1].into(),
            upstream: if f[2].is_empty() { None } else { Some(f[2].into()) },
            ahead,
            behind,
            gone: f[3] == "gone",
            current: f[4] == "*",
            time: f[5].parse().unwrap_or(0),
            worktree: if f[6].is_empty() { None } else { Some(f[6].into()) },
            subject: f[7].into(),
        });
    }
    Ok(out)
}

pub fn remotes(repo: &Path) -> R<Vec<String>> {
    Ok(run_s(repo, &["remote"])?.lines().map(String::from).filter(|s| !s.is_empty()).collect())
}

pub fn checkout(repo: &Path, name: &str, kind: &str) -> R<String> {
    not_option("branch", name)?;
    if kind == "remote" {
        let local = name.splitn(2, '/').nth(1).unwrap_or(name);
        if run(repo, &["rev-parse", "--verify", "-q", &format!("refs/heads/{local}")]).is_ok() {
            return run_op(repo, &["checkout", local]);
        }
        return run_op(repo, &["checkout", "-b", local, "--track", name]);
    }
    if kind == "tag" || kind == "commit" {
        return run_op(repo, &["checkout", "--detach", name]);
    }
    run_op(repo, &["checkout", name])
}

pub fn create_branch(repo: &Path, name: &str, start: &str, do_checkout: bool) -> R<String> {
    not_option("branch name", name)?;
    let start = if start.is_empty() { "HEAD" } else { not_option("start point", start)? };
    if do_checkout {
        run_op(repo, &["checkout", "-b", name, start])
    } else {
        run_op(repo, &["branch", name, start])
    }
}

pub fn delete_branch(repo: &Path, name: &str, kind: &str, force: bool) -> R<String> {
    match kind {
        "remote" => {
            let mut it = name.splitn(2, '/');
            let (r, b) = (not_option("remote", it.next().unwrap_or(""))?, it.next().unwrap_or(""));
            if b.is_empty() {
                return Err(format!("{name}: not a remote branch"));
            }
            run_op(repo, &["push", r, "--delete", &format!("refs/heads/{b}")])
        }
        "tag" => run_op(repo, &["tag", "-d", "--", name]),
        _ => run_op(repo, &["branch", if force { "-D" } else { "-d" }, "--", name]),
    }
}

pub fn rename_branch(repo: &Path, old: &str, new: &str) -> R<String> {
    run_op(repo, &["branch", "-m", "--", old, not_option("branch name", new)?])
}

pub fn merge(repo: &Path, rev: &str) -> R<String> {
    run_op(repo, &["merge", "--no-edit", not_option("revision", rev)?])
}

pub fn rebase(repo: &Path, onto: &str) -> R<String> {
    run_op(repo, &["rebase", not_option("revision", onto)?])
}

/// op: merge | rebase | cherry-pick | revert ; action: continue | abort | skip
pub fn sequencer(repo: &Path, op: &str, action: &str) -> R<String> {
    if op == "merge" {
        return match action {
            "abort" => run_op(repo, &["merge", "--abort"]),
            "continue" => run_op(repo, &["commit", "--no-edit"]),
            _ => Err(format!("unknown merge action: {action}")),
        };
    }
    if !["rebase", "cherry-pick", "revert"].contains(&op) || !["continue", "abort", "skip"].contains(&action) {
        return Err(format!("unknown sequencer operation: {op} {action}"));
    }
    run_op(repo, &[op, &format!("--{action}")])
}

pub fn cherry_pick(repo: &Path, hash: &str) -> R<String> {
    not_option("commit", hash)?;
    if commit_parents(repo, hash)?.len() > 1 {
        run_op(repo, &["cherry-pick", "-m", "1", hash])
    } else {
        run_op(repo, &["cherry-pick", hash])
    }
}

pub fn revert(repo: &Path, hash: &str) -> R<String> {
    not_option("commit", hash)?;
    if commit_parents(repo, hash)?.len() > 1 {
        run_op(repo, &["revert", "--no-edit", "-m", "1", hash])
    } else {
        run_op(repo, &["revert", "--no-edit", hash])
    }
}

pub fn reset(repo: &Path, hash: &str, mode: &str) -> R<String> {
    let m = match mode {
        "soft" => "--soft",
        "hard" => "--hard",
        "keep" => "--keep",
        "mixed" => "--mixed",
        _ => return Err(format!("unknown reset mode: {mode}")),
    };
    run_op(repo, &["reset", m, not_option("commit", hash)?, "--"])
}

pub fn create_tag(repo: &Path, name: &str, rev: &str, message: &str) -> R<String> {
    not_option("tag name", name)?;
    not_option("revision", rev)?;
    if message.trim().is_empty() {
        run_op(repo, &["tag", name, rev])
    } else {
        run_op(repo, &["tag", "-a", name, "-m", message, rev])
    }
}

// ───────────────────────────── remote ops ─────────────────────────────

pub fn fetch(repo: &Path) -> R<String> {
    run_op(repo, &["fetch", "--all", "--prune", "--progress"])
}

pub fn pull(repo: &Path, rebase: bool) -> R<String> {
    if rebase {
        run_op(repo, &["pull", "--rebase"])
    } else {
        run_op(repo, &["pull", "--no-rebase", "--no-edit"])
    }
}

/// Push `branch` (None = current). Sets upstream when missing.
pub fn push(repo: &Path, branch: Option<&str>, force: bool, tags: bool) -> R<String> {
    let current = run_s(repo, &["symbolic-ref", "--short", "-q", "HEAD"]).ok().map(|s| s.trim().to_string());
    let br = match branch.map(String::from).or(current) {
        Some(b) if !b.is_empty() => b,
        _ => return Err("HEAD is detached — create a branch first.".into()),
    };
    let cfg = |k: &str| run_s(repo, &["config", "--get", &format!("branch.{br}.{k}")]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let (up_remote, up_merge) = (cfg("remote"), cfg("merge"));
    let mut a: Vec<String> = vec!["push".into(), "--porcelain".into()];
    if force {
        a.push("--force-with-lease".into());
    }
    if tags {
        a.push("--follow-tags".into());
    }
    match (up_remote, up_merge) {
        // Remote names may contain '/', so read the config instead of splitting `@{upstream}`.
        (Some(remote), Some(merge)) if remote != "." => {
            a.push(remote);
            a.push(format!("refs/heads/{br}:{merge}"));
        }
        (local_upstream, _) => {
            let rs = remotes(repo)?;
            let remote = if rs.iter().any(|r| r == "origin") { "origin".to_string() } else { rs.first().cloned().ok_or("No remote configured")? };
            // Publishing: set upstream, unless the branch deliberately tracks a local branch.
            if local_upstream.is_none() {
                a.push("-u".into());
            }
            a.push(remote);
            a.push(format!("refs/heads/{br}:refs/heads/{br}"));
        }
    }
    let args: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
    run_op(repo, &args)
}

// ───────────────────────────── stash ─────────────────────────────

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub r#ref: String,
    pub time: i64,
    pub message: String,
}

pub fn stash_list(repo: &Path) -> R<Vec<StashEntry>> {
    let raw = run_s(repo, &["stash", "list", "--format=%gd%x1f%ct%x1f%gs"])?;
    Ok(raw
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(3, '\x1f').collect();
            (f.len() == 3).then(|| StashEntry { r#ref: f[0].into(), time: f[1].parse().unwrap_or(0), message: f[2].into() })
        })
        .collect())
}

pub fn stash_push(repo: &Path, message: &str, untracked: bool) -> R<String> {
    let mut a = vec!["stash", "push"];
    if untracked {
        a.push("--include-untracked");
    }
    if !message.trim().is_empty() {
        a.push("-m");
        a.push(message);
    }
    run_op(repo, &a)
}

pub fn stash_action(repo: &Path, action: &str, r: &str) -> R<String> {
    match action {
        "pop" | "apply" | "drop" => run_op(repo, &["stash", action, not_option("stash", r)?]),
        _ => Err("unknown stash action".into()),
    }
}

// ───────────────────────────── conflicts ─────────────────────────────

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConflictVersions {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub working: Option<String>,
    pub binary: bool,
    pub ours_label: String,
    pub theirs_label: String,
}

pub fn conflict_versions(repo: &Path, path: &str) -> R<ConflictVersions> {
    let stage = |n: u8| run(repo, &["show", &format!(":{n}:{path}")]).ok();
    let (b, o, t) = (stage(1), stage(2), stage(3));
    let w = std::fs::read(in_repo(repo, path)?).ok();
    // Non-UTF-8 text (Latin-1, GBK…) would be corrupted by the lossy round
    // trip through the editor, so treat it like binary: pick a side only.
    let binary = [&b, &o, &t, &w].iter().any(|x| x.as_ref().is_some_and(|v| v.contains(&0) || std::str::from_utf8(v).is_err()));
    let st = repo_state(repo)?;
    let s = |v: Option<Vec<u8>>| v.map(|x| lossy(&x));
    Ok(ConflictVersions {
        base: s(b),
        ours: s(o),
        theirs: s(t),
        working: s(w),
        binary,
        ours_label: st.ours_label,
        theirs_label: st.theirs_label,
    })
}

pub fn resolve_conflict(repo: &Path, path: &str, content: &str) -> R<String> {
    std::fs::write(in_repo(repo, path)?, content.as_bytes()).map_err(|e| e.to_string())?;
    run_op(repo, &["add", "--", path])
}

pub fn accept_side(repo: &Path, path: &str, side: &str) -> R<String> {
    let n = match side {
        "ours" => 2,
        "theirs" => 3,
        _ => return Err(format!("unknown side: {side}")),
    };
    if run(repo, &["cat-file", "-e", &format!(":{n}:{path}")]).is_ok() {
        run_op(repo, &["checkout", &format!("--{}", if side == "ours" { "ours" } else { "theirs" }), "--", path])?;
        run_op(repo, &["add", "--", path])
    } else {
        run_op(repo, &["rm", "-q", "--", path])
    }
}

// ───────────────────────────── worktrees ─────────────────────────────

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    /// short branch name; None when detached / bare
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    /// the main working tree (first entry of `git worktree list`)
    pub main: bool,
    /// the worktree `repo` itself points at
    pub current: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

fn is_linked_worktree(repo: &Path) -> bool {
    match run_s(repo, &["rev-parse", "--absolute-git-dir", "--git-common-dir"]) {
        Ok(s) => {
            let mut l = s.lines();
            let (gd, common) = (l.next().unwrap_or(""), l.next().unwrap_or(""));
            let common = if Path::new(common).is_absolute() { PathBuf::from(common) } else { repo.join(common) };
            let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            canon(Path::new(gd)) != canon(&common)
        }
        Err(_) => false,
    }
}

fn same_path(a: &str, b: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    canon(a) == canon(b)
}

pub fn parse_worktrees(raw: &[u8], current: &str) -> Vec<Worktree> {
    let mut out: Vec<Worktree> = Vec::new();
    for rec in raw.split(|&b| b == 0).collect::<Vec<_>>().split(|l| l.is_empty()) {
        let mut w = Worktree::default();
        for line in rec {
            let line = lossy(line);
            let (k, v) = line.split_once(' ').map(|(k, v)| (k, Some(v.to_string()))).unwrap_or((&line, None));
            match k {
                "worktree" => w.path = v.unwrap_or_default(),
                "HEAD" => w.head = v,
                "branch" => w.branch = v.map(|b| b.trim_start_matches("refs/heads/").to_string()),
                "bare" => w.bare = true,
                "detached" => w.detached = true,
                "locked" => w.locked = Some(v.unwrap_or_default()),
                "prunable" => w.prunable = Some(v.unwrap_or_default()),
                _ => {}
            }
        }
        if w.path.is_empty() {
            continue;
        }
        w.main = out.is_empty();
        w.current = same_path(&w.path, current);
        out.push(w);
    }
    out
}

pub fn worktree_list(repo: &Path) -> R<Vec<Worktree>> {
    let raw = run(repo, &["worktree", "list", "--porcelain", "-z"])?;
    Ok(parse_worktrees(&raw, &repo.to_string_lossy()))
}

/// mode: "new" (create `branch` from `start`), "existing" (check out local `branch`),
/// "remote" (create local tracking branch `branch` from remote `start`), "detached" (at `start`).
pub fn worktree_add(repo: &Path, path: &str, mode: &str, branch: &str, start: &str) -> R<String> {
    if path.trim().is_empty() {
        return Err("Worktree path is empty".into());
    }
    not_option("worktree path", path)?;
    let start = if start.trim().is_empty() { "HEAD" } else { not_option("start point", start.trim())? };
    if mode != "detached" {
        not_option("branch", branch)?;
    }
    match mode {
        "new" => run_op(repo, &["worktree", "add", "-b", branch, path, start]),
        "remote" => run_op(repo, &["worktree", "add", "--track", "-b", branch, path, start]),
        "existing" => run_op(repo, &["worktree", "add", path, branch]),
        "detached" => run_op(repo, &["worktree", "add", "--detach", path, start]),
        _ => Err("unknown worktree mode".into()),
    }
}

pub fn worktree_action(repo: &Path, action: &str, path: &str, arg: &str) -> R<String> {
    match action {
        "remove" => run_op(repo, &["worktree", "remove", path]),
        "force-remove" => run_op(repo, &["worktree", "remove", "--force", "--force", path]),
        "lock" if arg.trim().is_empty() => run_op(repo, &["worktree", "lock", path]),
        "lock" => run_op(repo, &["worktree", "lock", "--reason", arg, path]),
        "unlock" => run_op(repo, &["worktree", "unlock", path]),
        "move" => run_op(repo, &["worktree", "move", path, not_option("destination", arg)?]),
        "prune" => run_op(repo, &["worktree", "prune", "-v"]),
        _ => Err("unknown worktree action".into()),
    }
}

// ───────────────────────────── discovery ─────────────────────────────

pub fn scan_repos(root: &Path, depth: usize) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) {
        if dir.join(".git").exists() {
            out.push(dir.to_string_lossy().into_owned());
            return;
        }
        if depth == 0 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || ["node_modules", "target", "vendor", "build", "dist"].contains(&name.as_str()) {
                continue;
            }
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                walk(&e.path(), depth - 1, out);
            }
        }
    }
    walk(root, depth, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sh(repo: &Path, args: &[&str]) {
        run(repo, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    }

    fn tmp_repo(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("stashwick-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        sh(&d, &["init", "-q", "-b", "main"]);
        sh(&d, &["config", "user.name", "T"]);
        sh(&d, &["config", "user.email", "t@example.com"]);
        sh(&d, &["config", "commit.gpgsign", "false"]);
        sh(&d, &["config", "core.autocrlf", "false"]);
        d
    }

    #[test]
    fn worktrees() {
        let r = tmp_repo("wt");
        fs::write(r.join("a.txt"), "a\n").unwrap();
        stage(&r, &["a.txt".into()]).unwrap();
        commit(&r, "first", false, false).unwrap();
        let wt = r.with_file_name(format!("{}-feat", r.file_name().unwrap().to_string_lossy()));
        let _ = fs::remove_dir_all(&wt);
        let wts = wt.to_string_lossy().into_owned();
        worktree_add(&r, &wts, "new", "feat", "main").unwrap();
        let l = worktree_list(&r).unwrap();
        assert_eq!(l.len(), 2);
        assert!(l[0].main && l[0].current && l[0].branch.as_deref() == Some("main"));
        assert!(!l[1].main && !l[1].current && l[1].branch.as_deref() == Some("feat"));
        assert!(is_linked_worktree(&wt) && !is_linked_worktree(&r));
        let b = branches(&r).unwrap();
        let feat = b.iter().find(|b| b.name == "feat").unwrap();
        assert!(same_path(feat.worktree.as_deref().unwrap(), &wts));
        worktree_action(&r, "lock", &wts, "usb drive").unwrap();
        assert_eq!(worktree_list(&r).unwrap()[1].locked.as_deref(), Some("usb drive"));
        worktree_action(&r, "unlock", &wts, "").unwrap();
        fs::write(wt.join("dirty.txt"), "x").unwrap();
        assert!(worktree_action(&r, "remove", &wts, "").is_err());
        worktree_action(&r, "force-remove", &wts, "").unwrap();
        assert_eq!(worktree_list(&r).unwrap().len(), 1);
    }

    #[test]
    fn status_log_and_line_staging() {
        let r = tmp_repo("stage");
        // empty repo log
        assert!(log(&r, &LogQuery::default()).unwrap().is_empty());
        fs::write(r.join("a.txt"), "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n").unwrap();
        fs::write(r.join("new file.txt"), "x\n").unwrap();
        let st = status(&r).unwrap();
        assert_eq!(st.files.len(), 2);
        assert!(st.files.iter().all(|f| f.untracked));
        stage(&r, &["a.txt".into(), "new file.txt".into()]).unwrap();
        commit(&r, "first\n\nbody", false, false).unwrap();

        fs::write(r.join("a.txt"), "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\nELEVEN\n12\n").unwrap();
        let d = file_diff(&r, "a.txt", None, false, false).unwrap();
        assert_eq!(d.hunks.len(), 2);
        // stage only the second hunk's changes
        let sel: Vec<LineSel> = d.hunks[1]
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind != ' ')
            .map(|(i, _)| LineSel { hunk: 1, line: i })
            .collect();
        apply_lines(&r, "a.txt", None, "stage", &d.fingerprint, sel).unwrap();
        let staged = file_diff(&r, "a.txt", None, true, false).unwrap();
        assert_eq!(staged.hunks.len(), 1);
        assert!(staged.hunks[0].lines.iter().any(|l| l.text == "ELEVEN"));
        let unstaged = file_diff(&r, "a.txt", None, false, false).unwrap();
        assert_eq!(unstaged.hunks.len(), 1);
        assert!(unstaged.hunks[0].lines.iter().any(|l| l.text == "TWO"));

        // unstage single '+' line (and its '-') back
        let sel: Vec<LineSel> = staged.hunks[0]
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind != ' ')
            .map(|(i, _)| LineSel { hunk: 0, line: i })
            .collect();
        apply_lines(&r, "a.txt", None, "unstage", &staged.fingerprint, sel).unwrap();
        assert!(file_diff(&r, "a.txt", None, true, false).unwrap().hunks.is_empty());

        // discard just the "TWO" change
        let d = file_diff(&r, "a.txt", None, false, false).unwrap();
        let sel: Vec<LineSel> = d.hunks[0].lines.iter().enumerate().filter(|(_, l)| l.kind != ' ').map(|(i, _)| LineSel { hunk: 0, line: i }).collect();
        apply_lines(&r, "a.txt", None, "discard", &d.fingerprint, sel).unwrap();
        assert!(fs::read_to_string(r.join("a.txt")).unwrap().contains("\n2\n"));
        assert!(fs::read_to_string(r.join("a.txt")).unwrap().contains("ELEVEN"));

        let commits = log(&r, &LogQuery::default()).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].refs[0].name, "main");
        assert!(commits[0].refs[0].current);
        let det = commit_detail(&r, &commits[0].hash).unwrap();
        assert_eq!(det.files.len(), 2);
        assert_eq!(det.message, "first\n\nbody");
        let fd = commit_file_diff(&r, &commits[0].hash, "a.txt", None).unwrap();
        assert_eq!(fd.hunks[0].lines.len(), 12);
        fs::remove_dir_all(&r).ok();
    }

    #[test]
    fn merge_conflict_flow() {
        let r = tmp_repo("conflict");
        fs::write(r.join("f.txt"), "a\nb\nc\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "base", false, false).unwrap();
        create_branch(&r, "feature", "", true).unwrap();
        fs::write(r.join("f.txt"), "a\nFEATURE\nc\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "feat", false, false).unwrap();
        checkout(&r, "main", "local").unwrap();
        fs::write(r.join("f.txt"), "a\nMAIN\nc\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "main change", false, false).unwrap();

        assert!(merge(&r, "feature").is_err());
        let st = status(&r).unwrap();
        assert_eq!(st.state.kind, "merging");
        assert!(st.files[0].conflict);
        let d = file_diff(&r, "f.txt", None, false, false).unwrap();
        assert!(d.note.is_some()); // combined diff not parsed
        let v = conflict_versions(&r, "f.txt").unwrap();
        assert_eq!(v.base.as_deref(), Some("a\nb\nc\n"));
        assert_eq!(v.ours.as_deref(), Some("a\nMAIN\nc\n"));
        assert_eq!(v.theirs.as_deref(), Some("a\nFEATURE\nc\n"));
        assert!(v.theirs_label.contains("feature"));
        resolve_conflict(&r, "f.txt", "a\nMAIN+FEATURE\nc\n").unwrap();
        sequencer(&r, "merge", "continue").unwrap();
        let st = status(&r).unwrap();
        assert_eq!(st.state.kind, "");
        let b = branches(&r).unwrap();
        assert!(b.iter().any(|x| x.name == "main" && x.current));
        let lg = log(&r, &LogQuery::default()).unwrap();
        assert_eq!(lg[0].parents.len(), 2);
        fs::remove_dir_all(&r).ok();
    }
    #[test]
    fn edge_cases_crlf_no_eol_new_file() {
        let r = tmp_repo("edge");
        sh(&r, &["config", "core.autocrlf", "false"]);
        fs::write(r.join("w.txt"), "one\r\ntwo\r\nthree\r\n").unwrap();
        fs::write(r.join("n.txt"), "x\ny").unwrap(); // no trailing newline
        stage(&r, &["w.txt".into(), "n.txt".into()]).unwrap();
        commit(&r, "init", false, false).unwrap();
        // CRLF: change two lines in separate hunks? one hunk; stage only first change
        fs::write(r.join("w.txt"), "ONE\r\ntwo\r\nTHREE\r\n").unwrap();
        let d = file_diff(&r, "w.txt", None, false, false).unwrap();
        let idx: Vec<usize> = d.hunks[0].lines.iter().enumerate().filter(|(_, l)| l.text == "one" || l.text == "ONE").map(|(i, _)| i).collect();
        apply_lines(&r, "w.txt", None, "stage", &d.fingerprint, idx.into_iter().map(|line| LineSel { hunk: 0, line }).collect()).unwrap();
        let idx_content = run_s(&r, &["show", ":w.txt"]).unwrap();
        assert_eq!(idx_content, "ONE\r\ntwo\r\nthree\r\n");
        // no-EOL: modify last line, stage it
        fs::write(r.join("n.txt"), "x\nY").unwrap();
        let d = file_diff(&r, "n.txt", None, false, false).unwrap();
        let idx: Vec<LineSel> = d.hunks[0].lines.iter().enumerate().filter(|(_, l)| l.kind == '+' || l.kind == '-').map(|(i, _)| LineSel { hunk: 0, line: i }).collect();
        apply_lines(&r, "n.txt", None, "stage", &d.fingerprint, idx).unwrap();
        assert_eq!(run_s(&r, &["show", ":n.txt"]).unwrap(), "x\nY");
        // staged new file: unstage only one of its lines
        fs::write(r.join("z.txt"), "l1\nl2\nl3\n").unwrap();
        stage(&r, &["z.txt".into()]).unwrap();
        let d = file_diff(&r, "z.txt", None, true, false).unwrap();
        let sel = vec![LineSel { hunk: 0, line: 1 }];
        apply_lines(&r, "z.txt", None, "unstage", &d.fingerprint, sel).unwrap();
        assert_eq!(run_s(&r, &["show", ":z.txt"]).unwrap(), "l1\nl3\n");
        // rollback a modified + an untracked file
        fs::write(r.join("u.txt"), "u").unwrap();
        let st = status(&r).unwrap();
        let files: Vec<FileStatus> = st.files.into_iter().filter(|f| f.path == "u.txt" || f.path == "w.txt").collect();
        rollback(&r, &files).unwrap();
        assert!(!r.join("u.txt").exists());
        assert_eq!(fs::read_to_string(r.join("w.txt")).unwrap(), "one\r\ntwo\r\nthree\r\n");
        fs::remove_dir_all(&r).ok();
    }

    fn sel_where(d: &FileDiff, pred: impl Fn(&patch::DiffLine) -> bool) -> Vec<LineSel> {
        d.hunks.iter().enumerate().flat_map(|(hi, h)| h.lines.iter().enumerate().filter(|(_, l)| pred(l)).map(move |(li, _)| LineSel { hunk: hi, line: li })).collect()
    }

    #[test]
    fn commit_keeps_hash_lines() {
        let r = tmp_repo("hash");
        fs::write(r.join("a"), "a\n").unwrap();
        stage(&r, &["a".into()]).unwrap();
        commit(&r, "#123 Fix crash\n\nbody", false, false).unwrap();
        assert_eq!(last_message(&r).unwrap().trim(), "#123 Fix crash\n\nbody");
        fs::remove_dir_all(&r).ok();
    }

    #[test]
    fn partial_next_to_missing_eol() {
        let r = tmp_repo("noeol");
        fs::write(r.join("n.txt"), "x\ny").unwrap();
        stage(&r, &["n.txt".into()]).unwrap();
        commit(&r, "init", false, false).unwrap();
        fs::write(r.join("n.txt"), "x\nY").unwrap();
        // stage only the addition: index gets both lines
        let d = file_diff(&r, "n.txt", None, false, false).unwrap();
        apply_lines(&r, "n.txt", None, "stage", &d.fingerprint, sel_where(&d, |l| l.kind == '+')).unwrap();
        assert_eq!(run_s(&r, &["show", ":n.txt"]).unwrap(), "x\ny\nY");
        // unstage only the addition again: back to HEAD
        let d = file_diff(&r, "n.txt", None, true, false).unwrap();
        let plus = sel_where(&d, |l| l.kind == '+' && l.text == "Y");
        apply_lines(&r, "n.txt", None, "unstage", &d.fingerprint, plus).unwrap();
        // only "Y" goes; the separate "-y / +y" change (adding y's newline) stays staged
        assert_eq!(run_s(&r, &["show", ":n.txt"]).unwrap(), "x\ny\n");
        // discard only the deletion: worktree gets the old line back, keeps the new one
        let d = file_diff(&r, "n.txt", None, false, false).unwrap();
        apply_lines(&r, "n.txt", None, "discard", &d.fingerprint, sel_where(&d, |l| l.kind == '-')).unwrap();
        assert_eq!(fs::read_to_string(r.join("n.txt")).unwrap(), "x\ny\nY");
        fs::remove_dir_all(&r).ok();
    }

    #[test]
    fn partial_unstage_keeps_rename() {
        let r = tmp_repo("rename");
        let body: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        fs::write(r.join("f.txt"), &body).unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "init", false, false).unwrap();
        sh(&r, &["mv", "f.txt", "g.txt"]);
        fs::write(r.join("g.txt"), body.replace("line 2\n", "LINE 2\n").replace("line 19\n", "LINE 19\n")).unwrap();
        stage(&r, &["g.txt".into()]).unwrap();
        let st = status(&r).unwrap();
        let g = st.files.iter().find(|f| f.path == "g.txt").unwrap();
        assert_eq!(g.index, 'R');
        let d = file_diff(&r, "g.txt", Some("f.txt"), true, false).unwrap();
        assert_eq!(d.hunks.len(), 2);
        let first: Vec<LineSel> = sel_where(&d, |l| l.kind != ' ').into_iter().filter(|s| s.hunk == 0).collect();
        apply_lines(&r, "g.txt", Some("f.txt"), "unstage", &d.fingerprint, first).unwrap();
        let st = status(&r).unwrap();
        let g = st.files.iter().find(|f| f.path == "g.txt").unwrap();
        assert_eq!(g.index, 'R', "{:?}", st.files);
        assert!(run_s(&r, &["show", ":g.txt"]).unwrap().contains("line 2\n"));
        assert!(run_s(&r, &["show", ":g.txt"]).unwrap().contains("LINE 19\n"));
        fs::remove_dir_all(&r).ok();
    }

    #[test]
    fn rejects_escaping_paths_and_option_names() {
        let r = tmp_repo("guard");
        fs::write(r.join("a"), "a\n").unwrap();
        stage(&r, &["a".into()]).unwrap();
        commit(&r, "init", false, false).unwrap();
        assert!(in_repo(&r, "/etc/passwd").is_err());
        assert!(in_repo(&r, "../x").is_err());
        assert!(in_repo(&r, "sub/../../x").is_err());
        assert!(in_repo(&r, "sub/file.txt").is_ok());
        assert!(resolve_conflict(&r, "/tmp/stashwick-should-not-exist", "x").is_err());
        assert!(file_diff(&r, "../../etc/hosts", None, false, true).is_err());
        assert!(create_tag(&r, "-d", "HEAD", "").is_err());
        assert!(create_branch(&r, "--force", "", false).is_err());
        assert!(sequencer(&r, "gc", "continue").is_err());
        assert!(reset(&r, "HEAD", "bogus").is_err());
        // untracked fingerprint is stable, so polling doesn't re-render
        fs::write(r.join("u.txt"), "u\n").unwrap();
        let a = file_diff(&r, "u.txt", None, false, true).unwrap();
        assert!(!a.fingerprint.is_empty());
        assert_eq!(a.fingerprint, file_diff(&r, "u.txt", None, false, true).unwrap().fingerprint);
        // force-delete detection relies on this (English) message
        create_branch(&r, "side", "", true).unwrap();
        fs::write(r.join("a"), "b\n").unwrap();
        stage(&r, &["a".into()]).unwrap();
        commit(&r, "side", false, false).unwrap();
        checkout(&r, "main", "local").unwrap();
        let e = delete_branch(&r, "side", "local", false).unwrap_err();
        assert!(e.contains("not fully merged"), "{e}");
        delete_branch(&r, "side", "local", true).unwrap();
        fs::remove_dir_all(&r).ok();
    }

    #[test]
    fn non_utf8_conflict_is_binary() {
        let r = tmp_repo("latin1");
        fs::write(r.join("f.txt"), b"a\nb\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "base", false, false).unwrap();
        create_branch(&r, "feature", "", true).unwrap();
        fs::write(r.join("f.txt"), b"a\ncaf\xe9\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "feat", false, false).unwrap();
        checkout(&r, "main", "local").unwrap();
        fs::write(r.join("f.txt"), b"a\nMAIN\n").unwrap();
        stage(&r, &["f.txt".into()]).unwrap();
        commit(&r, "main", false, false).unwrap();
        assert!(merge(&r, "feature").is_err());
        assert!(conflict_versions(&r, "f.txt").unwrap().binary);
        fs::remove_dir_all(&r).ok();
    }
}
