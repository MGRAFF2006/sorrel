//! Minimal, dependency-free line-level diff for the prototype `diff` command.
//!
//! Computes a longest-common-subsequence (LCS) over lines and emits unified
//! diff hunks. Reconstruction uses linear auxiliary space, preserving the
//! original equal-first/delete-on-tie path. CPU time remains quadratic in the
//! number of lines; a faster algorithm can replace it without changing output.

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

/// The original terminator of a source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    /// An unterminated final line.
    None,
}

/// A line entry within a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkLine {
    /// Whether the line is context, added, or removed.
    pub kind: LineKind,
    /// The line content without its LF or CRLF terminator.
    pub text: String,
    pub line_ending: LineEnding,
}

impl HunkLine {
    fn from_raw(kind: LineKind, raw: &str) -> Self {
        let (text, line_ending) = if let Some(text) = raw.strip_suffix("\r\n") {
            (text, LineEnding::CrLf)
        } else if let Some(text) = raw.strip_suffix('\n') {
            (text, LineEnding::Lf)
        } else {
            (raw, LineEnding::None)
        };
        Self {
            kind,
            text: text.to_owned(),
            line_ending,
        }
    }
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
    // Compare complete source segments so terminator-only edits remain changes.
    text.split_inclusive('\n').map(str::to_owned).collect()
}

/// Builds an LCS-based edit script between `old` and `new` line vectors.
fn edit_script(old: &[String], new: &[String]) -> Vec<Edit> {
    let mut edits = Vec::new();
    append_edits(old, new, &mut edits);
    edits
}

fn append_edits(old: &[String], new: &[String], edits: &mut Vec<Edit>) {
    match (old, new) {
        ([], _) => edits.extend(new.iter().cloned().map(Edit::Insert)),
        (_, []) => edits.extend(old.iter().cloned().map(Edit::Delete)),
        ([line], _) => {
            if let Some(index) = new.iter().position(|candidate| candidate == line) {
                edits.extend(new[..index].iter().cloned().map(Edit::Insert));
                edits.push(Edit::Equal(line.clone()));
                edits.extend(new[index + 1..].iter().cloned().map(Edit::Insert));
            } else {
                edits.push(Edit::Delete(line.clone()));
                edits.extend(new.iter().cloned().map(Edit::Insert));
            }
        }
        _ => {
            let middle = old.len() / 2;
            let crossing = canonical_crossing(old, new, middle);
            // The helper's scratch rows are dropped before either recursive call.
            append_edits(&old[..middle], &new[..crossing], edits);
            append_edits(&old[middle..], &new[crossing..], edits);
        }
    }
}

/// Finds where the original equal-first/delete-on-tie path first reaches
/// `middle`. Ordinary LCS split-score ties cannot preserve that exact path.
fn canonical_crossing(old: &[String], new: &[String], middle: usize) -> usize {
    let m = new.len();
    let mut below = vec![0usize; m + 1];
    let mut row = vec![0usize; m + 1];
    let mut below_crossing: Vec<usize> = (0..=m).collect();
    let mut row_crossing = vec![m; m + 1];
    for i in (0..old.len()).rev() {
        for j in (0..m).rev() {
            let equal = old[i] == new[j];
            let delete = below[j] >= row[j + 1];
            row[j] = if equal {
                below[j + 1] + 1
            } else {
                below[j].max(row[j + 1])
            };
            if i < middle {
                row_crossing[j] = if equal {
                    below_crossing[j + 1]
                } else if delete {
                    below_crossing[j]
                } else {
                    row_crossing[j + 1]
                };
            }
        }
        std::mem::swap(&mut below, &mut row);
        if i < middle {
            std::mem::swap(&mut below_crossing, &mut row_crossing);
        }
    }
    below_crossing[0]
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
                    lines.push(HunkLine::from_raw(LineKind::Context, text));
                    old_len += 1;
                    new_len += 1;
                }
                Edit::Delete(text) => {
                    lines.push(HunkLine::from_raw(LineKind::Removed, text));
                    old_len += 1;
                }
                Edit::Insert(text) => {
                    lines.push(HunkLine::from_raw(LineKind::Added, text));
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
            if line.line_ending == LineEnding::None {
                out.push_str("\\ No newline at end of file\n");
            } else if line.line_ending == LineEnding::CrLf && line.kind != LineKind::Context {
                out.push_str("\\ CRLF line ending\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The previous full-matrix implementation is the compatibility oracle.
    fn quadratic_reference(old: &[String], new: &[String]) -> Vec<Edit> {
        let n = old.len();
        let m = new.len();

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

    fn sequences(alphabet: &[&str], max_len: usize) -> Vec<Vec<String>> {
        let mut all = vec![Vec::new()];
        for len in 1..=max_len {
            for mut encoded in 0..alphabet.len().pow(u32::try_from(len).unwrap()) {
                all.push(
                    (0..len)
                        .map(|_| {
                            let index = encoded % alphabet.len();
                            encoded /= alphabet.len();
                            alphabet[index].to_owned()
                        })
                        .collect(),
                );
            }
        }
        all
    }

    #[test]
    fn linear_reconstruction_matches_original_ties_and_line_endings() {
        for (alphabet, max_len) in [
            (&["a\n", "b\n"][..], 6),
            (&["a\n", "a\r\n", "a", "b\n"][..], 3),
        ] {
            let cases = sequences(alphabet, max_len);
            for old in &cases {
                for new in &cases {
                    assert_eq!(
                        edit_script(old, new),
                        quadratic_reference(old, new),
                        "old={old:?}, new={new:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn repeated_line_ties_preserve_hunk_positions_and_order() {
        assert_eq!(
            render_unified(&hunks("a\nb\n", "b\na\n", 0)),
            "@@ -1,1 +1,0 @@\n-a\n@@ -3,0 +2,1 @@\n+a\n"
        );
        assert_eq!(
            render_unified(&hunks("a\na\n", "a\n", 3)),
            "@@ -1,2 +1,1 @@\n a\n-a\n"
        );
    }

    #[test]
    fn byte_distinct_line_endings_produce_hunks() {
        for (old, new) in [
            ("a\n", "a"),
            ("a", "a\n"),
            ("a\r\n", "a\n"),
            ("a\n", "a\r\n"),
            ("a\r\n", "a"),
            ("a\nb", "a\nb\n"),
            ("a\r\nb\r\n", "a\r\nb\n"),
        ] {
            assert!(!hunks(old, new, 3).is_empty(), "old={old:?}, new={new:?}");
        }
    }

    #[test]
    fn render_and_metadata_identify_missing_newline_and_crlf() {
        let result = hunks("a\r\n", "a", 3);
        assert_eq!(result[0].old_len, 1);
        assert_eq!(result[0].new_len, 1);
        assert_eq!(result[0].lines[0].text, "a");
        assert_eq!(result[0].lines[0].line_ending, LineEnding::CrLf);
        assert_eq!(result[0].lines[1].line_ending, LineEnding::None);
        assert_eq!(
            render_unified(&result),
            "@@ -1,1 +1,1 @@\n-a\n\\ CRLF line ending\n+a\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn splitting_preserves_empty_lines_and_bare_carriage_returns() {
        assert!(hunks("", "", 3).is_empty());
        assert!(hunks("a\r\n\r\n", "a\r\n\r\n", 3).is_empty());
        let blank = hunks("", "\n", 3);
        assert_eq!(blank[0].new_len, 1);
        assert_eq!(blank[0].lines[0].text, "");
        assert_eq!(blank[0].lines[0].line_ending, LineEnding::Lf);
        let bare = hunks("a\r", "a\r\n", 3);
        assert_eq!(bare[0].lines[0].text, "a\r");
        assert_eq!(bare[0].lines[0].line_ending, LineEnding::None);
        assert_eq!(bare[0].lines[1].text, "a");
        assert_eq!(bare[0].lines[1].line_ending, LineEnding::CrLf);
    }

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
