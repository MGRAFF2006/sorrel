//! Pure text three-way merge.
//!
//! [`merge3`] merges line-oriented UTF-8 inputs against a common base using an
//! LCS diff on each side. Regions changed on only one side take that side;
//! identical changes on both sides merge cleanly; overlapping different changes
//! become [`ConflictHunk`]s with Git-style conflict markers. Non-UTF-8 input is
//! reported as [`MergeOutcome::Binary`].

/// Outcome of [`merge3`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeOutcome {
    /// All regions merged without conflict.
    Merged(Vec<u8>),
    /// At least one overlapping divergent change.
    Conflicted {
        /// Full text with Git-style conflict markers.
        merged_with_markers: Vec<u8>,
        /// Structured conflict regions (base line indices are 0-based).
        hunks: Vec<ConflictHunk>,
    },
    /// At least one input was not valid UTF-8.
    Binary,
}

/// One conflict region produced by [`merge3`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictHunk {
    /// 0-based start line index in `base` (the first base line covered, or the
    /// insertion point when the base range is empty).
    pub base_start: usize,
    /// Base lines covered by the conflict (no trailing newline characters).
    pub base_lines: Vec<String>,
    /// Replacement lines from ours.
    pub ours_lines: Vec<String>,
    /// Replacement lines from theirs.
    pub theirs_lines: Vec<String>,
}

/// Merges `ours` and `theirs` against common ancestor `base`.
///
/// Returns [`MergeOutcome::Binary`] if any input is not valid UTF-8. Otherwise
/// splits each input into lines (without newline characters), diffs
/// `base → ours` and `base → theirs` with LCS, and merges the edit scripts.
/// A clean merge preserves whether the result ends with a trailing newline.
#[must_use]
pub fn merge3(base: &[u8], ours: &[u8], theirs: &[u8]) -> MergeOutcome {
    let (Ok(base_s), Ok(ours_s), Ok(theirs_s)) = (
        std::str::from_utf8(base),
        std::str::from_utf8(ours),
        std::str::from_utf8(theirs),
    ) else {
        return MergeOutcome::Binary;
    };

    let (base_lines, base_nl) = split_lines(base_s);
    let (ours_lines, ours_nl) = split_lines(ours_s);
    let (theirs_lines, theirs_nl) = split_lines(theirs_s);

    let our_changes = diff_changes(&base_lines, &ours_lines);
    let their_changes = diff_changes(&base_lines, &theirs_lines);

    let mut out_lines: Vec<String> = Vec::new();
    let mut hunks: Vec<ConflictHunk> = Vec::new();
    let mut conflicted = false;

    let mut base_i = 0usize;
    let mut oi = 0usize;
    let mut ti = 0usize;

    while base_i < base_lines.len() || oi < our_changes.len() || ti < their_changes.len() {
        let next_our = our_changes.get(oi).map(|c| c.a0);
        let next_their = their_changes.get(ti).map(|c| c.a0);

        let next_change = match (next_our, next_their) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => Some(a.min(b)),
        };

        match next_change {
            None => {
                out_lines.extend(base_lines[base_i..].iter().cloned());
                break;
            }
            Some(start) if start > base_i => {
                out_lines.extend(base_lines[base_i..start].iter().cloned());
                base_i = start;
            }
            Some(_) => {
                // Grow an unstable region while either side has a change that
                // starts at or before the current region end (adjacent/overlapping).
                let mut region_a0 = base_i;
                let mut region_a1 = base_i;
                let mut saw_ours = false;
                let mut saw_theirs = false;

                loop {
                    let mut grew = false;
                    if oi < our_changes.len() && our_changes[oi].a0 <= region_a1 {
                        let c = our_changes[oi];
                        region_a0 = region_a0.min(c.a0);
                        region_a1 = region_a1.max(c.a1);
                        saw_ours = true;
                        oi += 1;
                        grew = true;
                    }
                    if ti < their_changes.len() && their_changes[ti].a0 <= region_a1 {
                        let c = their_changes[ti];
                        region_a0 = region_a0.min(c.a0);
                        region_a1 = region_a1.max(c.a1);
                        saw_theirs = true;
                        ti += 1;
                        grew = true;
                    }
                    if !grew {
                        break;
                    }
                }

                let ours_slice = extract_side(&ours_lines, &our_changes, region_a0, region_a1);
                let theirs_slice =
                    extract_side(&theirs_lines, &their_changes, region_a0, region_a1);

                if saw_ours && !saw_theirs {
                    out_lines.extend(ours_slice);
                } else if saw_theirs && !saw_ours {
                    out_lines.extend(theirs_slice);
                } else if ours_slice == theirs_slice {
                    out_lines.extend(ours_slice);
                } else {
                    conflicted = true;
                    hunks.push(ConflictHunk {
                        base_start: region_a0,
                        base_lines: base_lines[region_a0..region_a1].to_vec(),
                        ours_lines: ours_slice.clone(),
                        theirs_lines: theirs_slice.clone(),
                    });
                    out_lines.push("<<<<<<< ours".to_owned());
                    out_lines.extend(ours_slice);
                    out_lines.push("=======".to_owned());
                    out_lines.extend(theirs_slice);
                    out_lines.push(">>>>>>> theirs".to_owned());
                }

                base_i = region_a1;
            }
        }
    }

    let trailing_nl = merge_trailing_newline(base_nl, ours_nl, theirs_nl, conflicted);
    let bytes = join_lines(&out_lines, trailing_nl);

    if conflicted {
        MergeOutcome::Conflicted {
            merged_with_markers: bytes,
            hunks,
        }
    } else {
        MergeOutcome::Merged(bytes)
    }
}

/// Replacement of `base[a0..a1)` with `side[b0..b1)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Change {
    a0: usize,
    a1: usize,
    b0: usize,
    b1: usize,
}

fn split_lines(text: &str) -> (Vec<String>, bool) {
    if text.is_empty() {
        return (Vec::new(), false);
    }
    let trailing_newline = text.ends_with('\n');
    let body = text.strip_suffix('\n').unwrap_or(text);
    let lines = body.split('\n').map(str::to_owned).collect();
    (lines, trailing_newline)
}

fn join_lines(lines: &[String], trailing_newline: bool) -> Vec<u8> {
    if lines.is_empty() {
        return if trailing_newline {
            b"\n".to_vec()
        } else {
            Vec::new()
        };
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out.into_bytes()
}

fn merge_trailing_newline(base: bool, ours: bool, theirs: bool, conflicted: bool) -> bool {
    if conflicted {
        // Marker blocks are line-oriented; terminate like typical conflict files.
        return true;
    }
    let ours_changed = ours != base;
    let theirs_changed = theirs != base;
    match (ours_changed, theirs_changed) {
        (false, false) => base,
        (true, false) => ours,
        (false, true) => theirs,
        (true, true) => ours,
    }
}

/// Side index at the gap just before `base[idx]`, excluding inserts at `idx`.
fn map_before(changes: &[Change], idx: usize) -> usize {
    let mut s = idx;
    for c in changes {
        if c.a1 < idx || (c.a1 == idx && c.a0 < idx) {
            s = s - (c.a1 - c.a0) + (c.b1 - c.b0);
        } else {
            break;
        }
    }
    s
}

/// Side lines that correspond to base range `[a0, a1)`, including inserts at
/// `a0` and inside the range, but not inserts at `a1` (unless `a0 == a1`).
fn extract_side(side: &[String], changes: &[Change], a0: usize, a1: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut base_cursor = a0;
    let mut side_pos = map_before(changes, a0);

    for c in changes {
        // Fully before the range.
        if c.a1 < a0 || (c.a1 == a0 && c.a0 < a0) {
            continue;
        }
        // Fully after the range. Inserts at `a1` belong to the next region
        // unless this region is an empty insertion point (`a0 == a1`).
        if c.a0 > a1 {
            break;
        }
        if c.a0 == a1 && c.a1 == a1 && a0 != a1 {
            break;
        }
        if c.a0 >= a1 && a0 != a1 {
            break;
        }

        if c.a0 > base_cursor {
            let n = c.a0 - base_cursor;
            result.extend(side[side_pos..side_pos + n].iter().cloned());
            base_cursor = c.a0;
        }

        result.extend(side[c.b0..c.b1].iter().cloned());
        side_pos = c.b1;
        base_cursor = c.a1.max(base_cursor);
    }

    if base_cursor < a1 {
        let n = a1 - base_cursor;
        result.extend(side[side_pos..side_pos + n].iter().cloned());
    }
    result
}

fn diff_changes(base: &[String], side: &[String]) -> Vec<Change> {
    let ops = lcs_opcodes(base, side);
    let mut changes: Vec<Change> = Vec::new();
    let mut a = 0usize;
    let mut b = 0usize;

    for op in ops {
        match op {
            Op::Equal(n) => {
                a += n;
                b += n;
            }
            Op::Delete(n) => {
                let a0 = a;
                a += n;
                if let Some(last) = changes.last_mut() {
                    // Insert then delete at the same point → replace.
                    if last.a0 == a0 && last.a1 == a0 && last.b1 == b {
                        last.a1 = a;
                        continue;
                    }
                    // Extend a prior delete/replace ending here.
                    if last.a1 == a0 && last.b1 == b {
                        last.a1 = a;
                        continue;
                    }
                }
                changes.push(Change {
                    a0,
                    a1: a,
                    b0: b,
                    b1: b,
                });
            }
            Op::Insert(n) => {
                let b0 = b;
                b += n;
                if let Some(last) = changes.last_mut() {
                    // Delete then insert at the same point → replace.
                    if last.a1 == a && last.b1 == b0 {
                        last.b1 = b;
                        continue;
                    }
                }
                changes.push(Change {
                    a0: a,
                    a1: a,
                    b0,
                    b1: b,
                });
            }
        }
    }
    changes
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Equal(usize),
    Delete(usize),
    Insert(usize),
}

fn lcs_opcodes(base: &[String], side: &[String]) -> Vec<Op> {
    let n = base.len();
    let m = side.len();
    // dp[i][j] = LCS length of base[..i] and side[..j]
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if base[i] == side[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut raw: Vec<Op> = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && base[i - 1] == side[j - 1] {
            raw.push(Op::Equal(1));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            raw.push(Op::Insert(1));
            j -= 1;
        } else {
            raw.push(Op::Delete(1));
            i -= 1;
        }
    }
    raw.reverse();

    // Coalesce adjacent runs of the same op kind.
    let mut ops = Vec::new();
    for op in raw {
        match (ops.last_mut(), op) {
            (Some(Op::Equal(n)), Op::Equal(k)) => *n += k,
            (Some(Op::Delete(n)), Op::Delete(k)) => *n += k,
            (Some(Op::Insert(n)), Op::Insert(k)) => *n += k,
            _ => ops.push(op),
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged_identity(bytes: &[u8]) -> Vec<u8> {
        match merge3(bytes, bytes, bytes) {
            MergeOutcome::Merged(v) => v,
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn clean_merge_non_overlapping_edits_both_sides() {
        let base = b"a\nb\nc\nd\n";
        let ours = b"a\nB\nc\nd\n";
        let theirs = b"a\nb\nc\nD\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nB\nc\nD\n"),
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn identical_edits_both_sides_merge_cleanly() {
        let base = b"a\nb\nc\n";
        let ours = b"a\nB\nc\n";
        let theirs = b"a\nB\nc\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nB\nc\n"),
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn conflict_when_same_region_differs() {
        let base = b"a\nb\nc\n";
        let ours = b"a\nours\nc\n";
        let theirs = b"a\ntheirs\nc\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Conflicted {
                merged_with_markers,
                hunks,
            } => {
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].base_start, 1);
                assert_eq!(hunks[0].base_lines, vec!["b".to_owned()]);
                assert_eq!(hunks[0].ours_lines, vec!["ours".to_owned()]);
                assert_eq!(hunks[0].theirs_lines, vec!["theirs".to_owned()]);
                let text = String::from_utf8(merged_with_markers).unwrap();
                assert!(text.contains("<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n"));
            }
            other => panic!("expected Conflicted, got {other:?}"),
        }
    }

    #[test]
    fn ours_only_change() {
        let base = b"a\nb\nc\n";
        let ours = b"a\nB\nc\n";
        let theirs = b"a\nb\nc\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nB\nc\n"),
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn theirs_only_change() {
        let base = b"a\nb\nc\n";
        let ours = b"a\nb\nc\n";
        let theirs = b"a\nb\nC\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nb\nC\n"),
            other => panic!("expected Merged, got {other:?}"),
        }
    }

    #[test]
    fn delete_vs_edit_same_lines_conflicts() {
        let base = b"a\nb\nc\n";
        let ours = b"a\nc\n"; // deleted b
        let theirs = b"a\nB\nc\n"; // edited b
        match merge3(base, ours, theirs) {
            MergeOutcome::Conflicted { hunks, .. } => {
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].base_lines, vec!["b".to_owned()]);
                assert!(hunks[0].ours_lines.is_empty());
                assert_eq!(hunks[0].theirs_lines, vec!["B".to_owned()]);
            }
            other => panic!("expected Conflicted, got {other:?}"),
        }
    }

    #[test]
    fn binary_input_returns_binary() {
        let base = b"ok\n";
        let ours = b"ok\n";
        let theirs = b"bad\xff\n";
        assert_eq!(merge3(base, ours, theirs), MergeOutcome::Binary);
        assert_eq!(merge3(b"\xff", ours, theirs), MergeOutcome::Binary);
        assert_eq!(merge3(base, b"\xff", theirs), MergeOutcome::Binary);
    }

    #[test]
    fn empty_base_two_different_additions_conflicts() {
        let base = b"";
        let ours = b"ours\n";
        let theirs = b"theirs\n";
        match merge3(base, ours, theirs) {
            MergeOutcome::Conflicted {
                merged_with_markers,
                hunks,
            } => {
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].base_start, 0);
                assert!(hunks[0].base_lines.is_empty());
                assert_eq!(hunks[0].ours_lines, vec!["ours".to_owned()]);
                assert_eq!(hunks[0].theirs_lines, vec!["theirs".to_owned()]);
                let text = String::from_utf8(merged_with_markers).unwrap();
                assert!(text.contains("<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n"));
            }
            other => panic!("expected Conflicted, got {other:?}"),
        }
    }

    #[test]
    fn trailing_newline_preservation() {
        // Unchanged trailing newline is kept.
        match merge3(b"a\nb\n", b"a\nB\n", b"a\nb\n") {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nB\n"),
            other => panic!("expected Merged, got {other:?}"),
        }
        // Absence of trailing newline is preserved on a clean ours-only edit.
        match merge3(b"a\nb", b"a\nB", b"a\nb") {
            MergeOutcome::Merged(v) => assert_eq!(v, b"a\nB"),
            other => panic!("expected Merged, got {other:?}"),
        }
        // Theirs-only removal of the final newline wins.
        match merge3(b"hello\n", b"hello\n", b"hello") {
            MergeOutcome::Merged(v) => assert_eq!(v, b"hello"),
            other => panic!("expected Merged, got {other:?}"),
        }
        // Identity round-trip.
        assert_eq!(merged_identity(b"x\n"), b"x\n");
        assert_eq!(merged_identity(b"x"), b"x");
        assert_eq!(merged_identity(b""), b"");
    }
}
