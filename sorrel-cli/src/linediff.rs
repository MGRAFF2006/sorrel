//! Minimal, dependency-free line-level diff for the prototype `diff` command.
//!
//! Trims unchanged edges before a bounded longest-common-subsequence (LCS).
//! Large changed regions fall back to a complete replacement instead of an
//! unbounded quadratic allocation.

/// A single line in a hunk, tagged by its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// Unchanged context line present in both sides.
    Context,
    /// Line only in the new side.
    Added,
    /// Line only in the old side.
    Removed,
}

/// A line entry within a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkLine {
    /// Whether the line is context, added, or removed.
    pub kind: LineKind,
    /// The line content (without trailing newline).
    pub text: String,
}

/// A contiguous group of changes with surrounding context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based start line in the old file.
    pub old_start: usize,
    /// Number of old lines covered.
    pub old_len: usize,
    /// 1-based start line in the new file.
    pub new_start: usize,
    /// Number of new lines covered.
    pub new_len: usize,
    /// Lines in this hunk.
    pub lines: Vec<HunkLine>,
}

/// One element of the line-by-line edit script.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    Equal(String),
    Insert(String),
    Delete(String),
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    // `lines()` drops a trailing newline's empty final element, which is the
    // behavior we want for diffing whole files.
    text.lines().map(str::to_owned).collect()
}

/// Builds an LCS-based edit script between `old` and `new` line vectors.
fn edit_script(old: &[String], new: &[String]) -> Vec<Edit> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut edits: Vec<_> = old[..prefix].iter().cloned().map(Edit::Equal).collect();
    edits.extend(changed_script(
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    ));
    edits.extend(old[old.len() - suffix..].iter().cloned().map(Edit::Equal));
    edits
}

fn changed_script(old: &[String], new: &[String]) -> Vec<Edit> {
    let n = old.len();
    let m = new.len();
    // ponytail: bounded LCS; replace a large changed region as a whole. Myers
    // can improve large patches when compactness justifies another algorithm.
    const MAX_CELLS: usize = 1_000_000;
    if n == 0
        || m == 0
        || n.checked_add(1)
            .and_then(|n| m.checked_add(1).and_then(|m| n.checked_mul(m)))
            .is_none_or(|cells| cells > MAX_CELLS)
    {
        return old
            .iter()
            .cloned()
            .map(Edit::Delete)
            .chain(new.iter().cloned().map(Edit::Insert))
            .collect();
    }

    // lcs[i][j] = length of LCS of old[i..] and new[j..].
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if old[i] == new[j] {
            edits.push(Edit::Equal(old[i].clone()));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            edits.push(Edit::Delete(old[i].clone()));
            i += 1;
        } else {
            edits.push(Edit::Insert(new[j].clone()));
            j += 1;
        }
    }
    while i < n {
        edits.push(Edit::Delete(old[i].clone()));
        i += 1;
    }
    while j < m {
        edits.push(Edit::Insert(new[j].clone()));
        j += 1;
    }
    edits
}

/// Computes unified-diff hunks between two text blobs with `context` lines of
/// surrounding context.
#[must_use]
pub fn hunks(old_text: &str, new_text: &str, context: usize) -> Vec<Hunk> {
    let old = split_lines(old_text);
    let new = split_lines(new_text);
    let edits = edit_script(&old, &new);

    // Index of each edit that represents a change (insert/delete).
    let changed: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| !matches!(edit, Edit::Equal(_)))
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }

    // Group changed edit indices into hunks separated by > 2*context equals.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = changed[0];
    let mut end = changed[0];
    for &index in &changed[1..] {
        if index - end <= 2 * context + 1 {
            end = index;
        } else {
            groups.push((start, end));
            start = index;
            end = index;
        }
    }
    groups.push((start, end));

    let mut hunks = Vec::new();
    for (group_start, group_end) in groups {
        let from = group_start.saturating_sub(context);
        let to = (group_end + context + 1).min(edits.len());

        let old_start = 1 + edits[..from]
            .iter()
            .filter(|edit| matches!(edit, Edit::Equal(_) | Edit::Delete(_)))
            .count();
        let new_start = 1 + edits[..from]
            .iter()
            .filter(|edit| matches!(edit, Edit::Equal(_) | Edit::Insert(_)))
            .count();

        let mut lines = Vec::new();
        let mut old_len = 0;
        let mut new_len = 0;
        for edit in &edits[from..to] {
            match edit {
                Edit::Equal(text) => {
                    lines.push(HunkLine {
                        kind: LineKind::Context,
                        text: text.clone(),
                    });
                    old_len += 1;
                    new_len += 1;
                }
                Edit::Delete(text) => {
                    lines.push(HunkLine {
                        kind: LineKind::Removed,
                        text: text.clone(),
                    });
                    old_len += 1;
                }
                Edit::Insert(text) => {
                    lines.push(HunkLine {
                        kind: LineKind::Added,
                        text: text.clone(),
                    });
                    new_len += 1;
                }
            }
        }

        hunks.push(Hunk {
            old_start,
            old_len,
            new_start,
            new_len,
            lines,
        });
    }

    hunks
}

/// Renders hunks as a unified-diff body (without file headers).
#[must_use]
pub fn render_unified(hunks: &[Hunk]) -> String {
    let mut out = String::new();
    for hunk in hunks {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
        ));
        for line in &hunk.lines {
            let prefix = match line.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            out.push(prefix);
            out.push_str(&line.text);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_hunks() {
        assert!(hunks("a\nb\nc\n", "a\nb\nc\n", 3).is_empty());
    }

    #[test]
    fn single_line_modification_produces_one_hunk() {
        let result = hunks("a\nb\nc\n", "a\nB\nc\n", 3);
        assert_eq!(result.len(), 1);
        let hunk = &result[0];
        assert!(hunk
            .lines
            .iter()
            .any(|line| line.kind == LineKind::Removed && line.text == "b"));
        assert!(hunk
            .lines
            .iter()
            .any(|line| line.kind == LineKind::Added && line.text == "B"));
    }

    #[test]
    fn pure_addition_at_end() {
        let result = hunks("a\n", "a\nb\n", 3);
        assert_eq!(result.len(), 1);
        assert!(result[0]
            .lines
            .iter()
            .any(|line| line.kind == LineKind::Added && line.text == "b"));
    }

    #[test]
    fn render_includes_hunk_header_and_signs() {
        let result = hunks("a\nb\n", "a\nc\n", 3);
        let rendered = render_unified(&result);
        assert!(rendered.contains("@@ -"));
        assert!(rendered.contains("-b"));
        assert!(rendered.contains("+c"));
        assert!(rendered.contains(" a"));
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;

    #[test]
    fn large_unchanged_edges_still_produce_a_compact_edit() {
        let mut old: Vec<_> = (0..20_000).map(|n| format!("line {n}\n")).collect();
        let original = old.concat();
        old[10_000] = "replacement\n".to_owned();
        let hunks = hunks(&original, &old.concat(), 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 8);
        assert_eq!(hunks[0].old_start, 9_998);
    }

    #[test]
    fn oversized_changed_region_replacement_preserves_every_line() {
        let old = (0..2_000).map(|n| format!("old {n}\n")).collect::<String>();
        let new = (0..2_000).map(|n| format!("new {n}\n")).collect::<String>();
        let hunks = hunks(&old, &new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_len, 2_000);
        assert_eq!(hunks[0].new_len, 2_000);
        assert!(hunks[0]
            .lines
            .iter()
            .any(|line| line.kind == LineKind::Removed && line.text == "old 1999"));
        assert!(hunks[0]
            .lines
            .iter()
            .any(|line| line.kind == LineKind::Added && line.text == "new 1999"));
    }
}
