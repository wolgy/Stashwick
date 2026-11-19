//! Unified-diff parsing and partial-patch construction (hunk / line staging).
//!
//! We keep the raw bytes of every diff line so that rebuilt patches apply
//! byte-for-byte (CRLF files, non-UTF-8 content), and only convert to text
//! for display.

use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct RawHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// Text after the closing `@@` (function context), raw.
    pub context: Vec<u8>,
    /// Lines including their 1-byte prefix (' ', '+', '-', '\\'), without '\n'.
    pub lines: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct RawDiff {
    pub header: Vec<Vec<u8>>,
    pub hunks: Vec<RawHunk>,
    pub binary: bool,
    pub combined: bool,
    pub fingerprint: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: char,
    pub text: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub too_large: bool,
    pub untracked: bool,
    pub fingerprint: String,
    pub note: Option<String>,
}

pub fn fingerprint(bytes: &[u8]) -> String {
    // FNV-1a 64
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn parse_range(s: &str) -> Option<(u32, u32)> {
    let mut it = s.splitn(2, ',');
    let start = it.next()?.parse().ok()?;
    let count = match it.next() {
        Some(c) => c.parse().ok()?,
        None => 1,
    };
    Some((start, count))
}

/// Parse `@@ -a,b +c,d @@ ctx`
fn parse_hunk_header(line: &[u8]) -> Option<(u32, u32, u32, u32, Vec<u8>)> {
    if !line.starts_with(b"@@ ") {
        return None;
    }
    let rest = &line[3..];
    let end = find(rest, b" @@")?;
    let ranges = std::str::from_utf8(&rest[..end]).ok()?;
    let ctx = rest[end + 3..].to_vec();
    let mut parts = ranges.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let (os, oc) = parse_range(old)?;
    let (ns, nc) = parse_range(new)?;
    Some((os, oc, ns, nc, ctx))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn parse(bytes: &[u8]) -> RawDiff {
    let mut d = RawDiff {
        fingerprint: fingerprint(bytes),
        ..Default::default()
    };
    let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let mut seen_diff_header = false;
    for line in lines {
        if line.starts_with(b"diff --cc ") || line.starts_with(b"diff --combined ") {
            d.combined = true;
            return d;
        }
        if line.starts_with(b"diff --git ") {
            if seen_diff_header {
                break; // only the first file pair
            }
            seen_diff_header = true;
        }
        if let Some((os, oc, ns, nc, ctx)) = parse_hunk_header(line) {
            d.hunks.push(RawHunk {
                old_start: os,
                old_count: oc,
                new_start: ns,
                new_count: nc,
                context: ctx,
                lines: vec![],
            });
            continue;
        }
        match d.hunks.last_mut() {
            None => {
                if line.starts_with(b"Binary files ") || line.starts_with(b"GIT binary patch") {
                    d.binary = true;
                }
                d.header.push(line.to_vec());
            }
            Some(h) => {
                let l = if line.is_empty() { b" ".to_vec() } else { line.to_vec() };
                h.lines.push(l);
            }
        }
    }
    d
}

fn display(bytes: &[u8]) -> String {
    let b = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    String::from_utf8_lossy(b).into_owned()
}

impl RawDiff {
    pub fn to_view(&self) -> FileDiff {
        let hunks = self
            .hunks
            .iter()
            .map(|h| {
                let mut o = h.old_start;
                let mut n = h.new_start;
                let lines = h
                    .lines
                    .iter()
                    .map(|l| {
                        let kind = l[0] as char;
                        let text = display(&l[1..]);
                        let (old_no, new_no) = match kind {
                            ' ' => {
                                o += 1;
                                n += 1;
                                (Some(o - 1), Some(n - 1))
                            }
                            '-' => {
                                o += 1;
                                (Some(o - 1), None)
                            }
                            '+' => {
                                n += 1;
                                (None, Some(n - 1))
                            }
                            _ => (None, None),
                        };
                        DiffLine { kind, text, old_no, new_no }
                    })
                    .collect();
                Hunk {
                    header: format!(
                        "@@ -{},{} +{},{} @@{}",
                        h.old_start,
                        h.old_count,
                        h.new_start,
                        h.new_count,
                        display(&h.context)
                    ),
                    old_start: h.old_start,
                    old_count: h.old_count,
                    new_start: h.new_start,
                    new_count: h.new_count,
                    lines,
                }
            })
            .collect();
        FileDiff {
            header: self.header.iter().map(|l| display(l)).collect(),
            hunks,
            binary: self.binary,
            fingerprint: self.fingerprint.clone(),
            note: if self.combined {
                Some("File has merge conflicts — use the conflict resolver.".into())
            } else {
                None
            },
            ..Default::default()
        }
    }

    fn change_count(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| l[0] == b'+' || l[0] == b'-')
            .count()
    }

    /// Header to use when only part of the changes is applied: creation /
    /// deletion headers would make `git apply` insist on the whole file, and a
    /// rename header would make a partial (reverse) apply undo the rename. We
    /// emit a plain modification of a single path instead (the new path for
    /// renames/copies, the old path for deletions).
    fn partial_header(&self) -> Vec<Vec<u8>> {
        let special = self.header.iter().any(|l| {
            l.starts_with(b"new file mode")
                || l.starts_with(b"deleted file mode")
                || l.starts_with(b"rename from")
                || l.starts_with(b"copy from")
        });
        if !special {
            return self.header.clone();
        }
        let minus = self.header.iter().find(|l| l.starts_with(b"--- ")).cloned();
        let plus = self.header.iter().find(|l| l.starts_with(b"+++ ")).cloned();
        let (Some(minus), Some(plus)) = (minus, plus) else {
            return self.header.clone();
        };
        let (old, new) = (&minus[4..], &plus[4..]);
        let side = if new != b"/dev/null" { new } else { old };
        // `a/path` or C-quoted `"a/path"`; keep the quoting, swap the prefix.
        let (quoted, path) = match side.first() {
            Some(b'"') => (true, side.get(3..).unwrap_or_default()),
            _ => (false, side.get(2..).unwrap_or_default()),
        };
        let name = |prefix: &[u8]| {
            let mut v = Vec::new();
            if quoted {
                v.push(b'"');
            }
            v.extend_from_slice(prefix);
            v.extend_from_slice(path);
            v
        };
        let line = |parts: &[&[u8]]| parts.concat();
        vec![
            line(&[b"diff --git ", &name(b"a/"), b" ", &name(b"b/")]),
            line(&[b"--- ", &name(b"a/")]),
            line(&[b"+++ ", &name(b"b/")]),
        ]
    }

    /// Build a patch containing only the selected `(hunk, line)` changes.
    ///
    /// `reverse == false`: patch is applied forward (staging from the
    /// worktree diff). `reverse == true`: patch will be applied with `-R`
    /// (unstaging from the cached diff, or discarding worktree lines).
    pub fn build_partial(&self, sel: &HashSet<(usize, usize)>, reverse: bool) -> Option<Vec<u8>> {
        let selected_changes = self
            .hunks
            .iter()
            .enumerate()
            .flat_map(|(hi, h)| {
                h.lines
                    .iter()
                    .enumerate()
                    .filter(move |(li, l)| (l[0] == b'+' || l[0] == b'-') && sel.contains(&(hi, *li)))
            })
            .count();
        if selected_changes == 0 {
            return None;
        }
        let complete = selected_changes == self.change_count();
        let header = if complete { self.header.clone() } else { self.partial_header() };

        let mut out: Vec<u8> = Vec::new();
        for l in &header {
            out.extend_from_slice(l);
            out.push(b'\n');
        }
        let mut delta: i64 = 0;
        for (hi, h) in self.hunks.iter().enumerate() {
            let has_sel = h
                .lines
                .iter()
                .enumerate()
                .any(|(li, l)| (l[0] == b'+' || l[0] == b'-') && sel.contains(&(hi, li)));
            if !has_sel {
                continue;
            }
            // (line incl. prefix, "\ No newline" marker that followed it)
            let mut body: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::new();
            let mut last_kept = false;
            for (li, l) in h.lines.iter().enumerate() {
                let selected = sel.contains(&(hi, li));
                let kept: Option<Vec<u8>> = match l[0] {
                    b' ' => Some(l.clone()),
                    b'+' if selected => Some(l.clone()),
                    b'-' if selected => Some(l.clone()),
                    // unselected: the side that already has the line keeps it as context
                    b'+' | b'-' if (l[0] == b'+') == reverse => {
                        let mut c = l.clone();
                        c[0] = b' ';
                        Some(c)
                    }
                    b'+' | b'-' => None,
                    b'\\' => {
                        if last_kept {
                            if let Some(last) = body.last_mut() {
                                last.1 = Some(l.clone());
                            }
                        }
                        continue;
                    }
                    _ => Some(l.clone()),
                };
                last_kept = kept.is_some();
                if let Some(k) = kept {
                    body.push((k, None));
                }
            }
            let body = fix_eof_markers(body);
            let (mut oc, mut nc) = (0u32, 0u32);
            for l in &body {
                match l[0] {
                    b' ' => {
                        oc += 1;
                        nc += 1
                    }
                    b'-' => oc += 1,
                    b'+' => nc += 1,
                    _ => {}
                }
            }
            let (os, ns) = if reverse {
                let ns = h.new_start as i64;
                ((ns - delta).max(0) as u32, ns as u32)
            } else {
                let os = h.old_start as i64;
                (os as u32, (os + delta).max(0) as u32)
            };
            delta += nc as i64 - oc as i64;
            out.extend_from_slice(format!("@@ -{os},{oc} +{ns},{nc} @@").as_bytes());
            out.extend_from_slice(&h.context);
            out.push(b'\n');
            for l in body {
                out.extend_from_slice(&l);
                out.push(b'\n');
            }
        }
        Some(out)
    }
}

/// A "\\ No newline at end of file" marker is only valid on the last line
/// of a side. Dropping unselected lines can leave a marked line that is no
/// longer last on one side (e.g. staging an addition after an unchanged,
/// unterminated last line), which `git apply` would accept and then join the
/// two lines. Rewrite such lines so each side ends where the marker says.
fn fix_eof_markers(body: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Vec<Vec<u8>> {
    let (mut later_old, mut later_new) = (false, false);
    let mut rev: Vec<Vec<u8>> = Vec::with_capacity(body.len() + 2);
    for (line, marker) in body.into_iter().rev() {
        let kind = line[0];
        let push = |rev: &mut Vec<Vec<u8>>, l: Vec<u8>, m: Option<Vec<u8>>| {
            // building in reverse: marker goes before its line
            if let Some(m) = m {
                rev.push(m);
            }
            rev.push(l);
        };
        match (kind, marker) {
            (b' ', Some(m)) if later_old != later_new => {
                let mut minus = line.clone();
                minus[0] = b'-';
                let mut plus = line;
                plus[0] = b'+';
                if later_new {
                    // old side ends here, new side continues
                    push(&mut rev, plus, None);
                    push(&mut rev, minus, Some(m));
                } else {
                    push(&mut rev, plus, Some(m));
                    push(&mut rev, minus, None);
                }
            }
            (b' ', m) => push(&mut rev, line, if later_old || later_new { None } else { m }),
            (b'-', m) => push(&mut rev, line, if later_old { None } else { m }),
            (b'+', m) => push(&mut rev, line, if later_new { None } else { m }),
            (_, m) => push(&mut rev, line, m),
        }
        match kind {
            b' ' => (later_old, later_new) = (true, true),
            b'-' => later_old = true,
            b'+' => later_new = true,
            _ => {}
        }
    }
    rev.reverse();
    rev
}

/// Synthesize a view for an untracked file (everything added).
pub fn untracked_view(content: &[u8]) -> FileDiff {
    let mut fd = FileDiff {
        untracked: true,
        header: vec!["new file (untracked)".into()],
        fingerprint: fingerprint(content),
        ..Default::default()
    };
    if content.contains(&0) {
        fd.binary = true;
        return fd;
    }
    let mut lines: Vec<&[u8]> = content.split(|b| *b == b'\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let n = lines.len() as u32;
    fd.hunks.push(Hunk {
        header: format!("@@ -0,0 +1,{n} @@"),
        old_start: 0,
        old_count: 0,
        new_start: 1,
        new_count: n,
        lines: lines
            .iter()
            .enumerate()
            .map(|(i, l)| DiffLine {
                kind: '+',
                text: display(l),
                old_no: None,
                new_no: Some(i as u32 + 1),
            })
            .collect(),
    });
    fd
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: &str = "diff --git a/f.txt b/f.txt\nindex 1..2 100644\n--- a/f.txt\n+++ b/f.txt\n@@ -1,4 +1,4 @@ ctx\n a\n-b\n+B\n c\n-d\n+D\n";

    #[test]
    fn parses() {
        let d = parse(D.as_bytes());
        assert_eq!(d.header.len(), 4);
        assert_eq!(d.hunks.len(), 1);
        assert_eq!(d.hunks[0].lines.len(), 6);
        let v = d.to_view();
        assert_eq!(v.hunks[0].lines[1].old_no, Some(2));
        assert_eq!(v.hunks[0].lines[2].new_no, Some(2));
    }

    #[test]
    fn partial_forward() {
        let d = parse(D.as_bytes());
        let sel: HashSet<_> = [(0, 1), (0, 2)].into_iter().collect();
        let p = String::from_utf8(d.build_partial(&sel, false).unwrap()).unwrap();
        assert!(p.contains("@@ -1,4 +1,4 @@ ctx\n a\n-b\n+B\n c\n d\n"), "{p}");
    }

    #[test]
    fn partial_reverse() {
        let d = parse(D.as_bytes());
        let sel: HashSet<_> = [(0, 4), (0, 5)].into_iter().collect();
        let p = String::from_utf8(d.build_partial(&sel, true).unwrap()).unwrap();
        assert!(p.contains("@@ -1,4 +1,4 @@ ctx\n a\n B\n c\n-d\n+D\n"), "{p}");
    }
}
