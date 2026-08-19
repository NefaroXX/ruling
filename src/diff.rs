//! Core line-diff engine for `verdict`.
//!
//! This module computes a line-based diff between two sequences of lines and
//! formats the result as GNU-style unified hunks with `@@ -a,b +c,d @@`
//! headers.

/// A single line in a unified diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// An unchanged (context) line.
    Context(String),
    /// A line present only in the old file.
    Removed(String),
    /// A line present only in the new file.
    Added(String),
}

/// A contiguous region of differences in a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based starting line in the old file.
    pub old_start: usize,
    /// Number of old-file lines in this hunk.
    pub old_count: usize,
    /// 1-based starting line in the new file.
    pub new_start: usize,
    /// Number of new-file lines in this hunk.
    pub new_count: usize,
    /// The context/removed/added lines making up this hunk.
    pub lines: Vec<DiffLine>,
}

/// Compute a list of unified hunks describing the differences between `old`
/// and `new`.
///
/// `context` is the number of unchanged lines to include around each change
/// (0 means no context). If `ignore_case` is true, lines are compared
/// case-insensitively for equality purposes.
///
/// The implementation uses the classic dynamic-programming (LCS) approach. A
/// full `(n+1) x (m+1)` DP table is retained so that reconstruction can walk
/// arbitrary rows; a rolling two-row table would corrupt the edit script
/// because reconstruction needs `dp[i+1][j]` and `dp[i][j+1]` at arbitrary
/// `i` as the walk advances.
pub fn compute_hunks(
    old: &[String],
    new: &[String],
    context: usize,
    ignore_case: bool,
) -> Vec<Hunk> {
    // Determine which line pairs are "equal" under the configured comparison.
    let eq = |a: &str, b: &str| {
        if ignore_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };

    // Compute the longest common subsequence (LCS) DP table. `dp[i][j]` is the
    // length of the LCS of `old[i..]` and `new[j..]`. We iterate from the
    // bottom-right corner toward the top-left.
    let n = old.len();
    let m = new.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if eq(&old[i], &new[j]) {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else {
                dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    // Reconstruct the edit script: mark each old line as removed and each new
    // line as added, walking the LCS table from the top-left corner. At each
    // mismatch we prefer the move that preserves the LCS value: if advancing
    // in the old sequence keeps (or exceeds) the LCS length we remove the old
    // line, otherwise we add the new line.
    let mut removed = vec![false; n];
    let mut added = vec![false; m];
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if eq(&old[i], &new[j]) {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            // Advancing in the old sequence preserves the LCS value.
            removed[i] = true;
            i += 1;
        } else {
            added[j] = true;
            j += 1;
        }
    }
    while i < n {
        removed[i] = true;
        i += 1;
    }
    while j < m {
        added[j] = true;
        j += 1;
    }

    // Group the changed line pairs into hunks, each expanded to include up to
    // `context` unchanged lines on both sides.
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut pos = 0usize;
    let max_len = n.max(m);

    while pos < max_len {
        // Scan for the next changed position.
        let mut change = None;
        for k in pos..max_len {
            if (k < n && removed[k]) || (k < m && added[k]) {
                change = Some(k);
                break;
            }
        }
        let Some(change_pos) = change else {
            break;
        };

        // Expand the hunk to cover all consecutive changes, plus trailing
        // context. `extent` is the furthest index (in either sequence) that
        // belongs to this hunk.
        let mut extent = change_pos;
        let mut cursor = change_pos;
        loop {
            // Advance `cursor` until we pass all changes within `context` of
            // the previous one, collecting the furthest changed index.
            let mut furthest_change = None;
            while cursor + 1 < max_len {
                cursor += 1;
                if (cursor < n && removed[cursor]) || (cursor < m && added[cursor]) {
                    furthest_change = Some(cursor);
                }
                // If we've moved more than `context` past the last change,
                // this hunk ends here.
                let last_change = furthest_change.unwrap_or(change_pos);
                if cursor > last_change + context {
                    break;
                }
            }
            match furthest_change {
                Some(fc) if fc > extent => {
                    extent = fc;
                }
                _ => break,
            }
        }

        // Determine the low and high bounds in each sequence. The low bound
        // backs up `context` lines before the first change; the high bound
        // extends `context` lines past the last change so trailing context is
        // included (matching GNU diff).
        let old_lo = change_pos.saturating_sub(context);
        let new_lo = change_pos.saturating_sub(context);
        let old_hi = extent.saturating_add(context).min(n.saturating_sub(1));
        let new_hi = extent.saturating_add(context).min(m.saturating_sub(1));

        // Emit the hunk lines by walking both sequences within bounds.
        let mut lines = Vec::new();
        let mut oi = old_lo;
        let mut ni = new_lo;
        while oi <= old_hi || ni <= new_hi {
            let in_old = oi < n;
            let in_new = ni < m;
            let old_removed = in_old && removed[oi];
            let new_added = in_new && added[ni];

            if old_removed && new_added && eq(&old[oi], &new[ni]) {
                // Both marked changed but equal (possible with ignore-case);
                // emit as context to keep output stable.
                lines.push(DiffLine::Context(old[oi].clone()));
                oi += 1;
                ni += 1;
            } else if old_removed {
                lines.push(DiffLine::Removed(old[oi].clone()));
                oi += 1;
            } else if new_added {
                lines.push(DiffLine::Added(new[ni].clone()));
                ni += 1;
            } else if in_old && in_new {
                lines.push(DiffLine::Context(old[oi].clone()));
                oi += 1;
                ni += 1;
            } else if in_old {
                lines.push(DiffLine::Removed(old[oi].clone()));
                oi += 1;
            } else if in_new {
                lines.push(DiffLine::Added(new[ni].clone()));
                ni += 1;
            } else {
                break;
            }
        }

        let old_count = lines
            .iter()
            .filter(|l| !matches!(l, DiffLine::Added(_)))
            .count();
        let new_count = lines
            .iter()
            .filter(|l| !matches!(l, DiffLine::Removed(_)))
            .count();

        hunks.push(Hunk {
            old_start: old_lo + 1,
            old_count,
            new_start: new_lo + 1,
            new_count,
            lines,
        });

        // Continue scanning after this hunk's extent.
        pos = extent + 1;
    }

    // Merge adjacent hunks whose ranges overlap. When two changes are close
    // together (within 2 * context lines), their context regions overlap and
    // GNU diff merges them into a single hunk.
    hunks = merge_adjacent_hunks(hunks, old, new);

    // Fix empty-file start positions: GNU diff uses 0 when a side has no lines.
    if old.is_empty() {
        for h in &mut hunks {
            h.old_start = 0;
        }
    }
    if new.is_empty() {
        for h in &mut hunks {
            h.new_start = 0;
        }
    }

    hunks
}

/// Merge adjacent hunks whose old-file or new-file ranges overlap.
///
/// When two changes are separated by fewer than `2 * context` lines, their
/// context regions overlap and GNU diff merges them into a single hunk. This
/// post-processing pass performs that merge.
fn merge_adjacent_hunks(hunks: Vec<Hunk>, old: &[String], new: &[String]) -> Vec<Hunk> {
    if hunks.len() <= 1 {
        return hunks;
    }
    let mut merged = Vec::with_capacity(hunks.len());
    let mut iter = hunks.into_iter();
    let mut current = iter.next().unwrap();

    for next in iter {
        let cur_old_end = current.old_start + current.old_count - 1;
        let cur_new_end = current.new_start + current.new_count - 1;
        let overlap = cur_old_end >= next.old_start && cur_new_end >= next.new_start;

        if overlap {
            // Merge: take the minimum start, rebuild lines from original ranges.
            let merged_old_start = current.old_start.min(next.old_start);
            let merged_new_start = current.new_start.min(next.new_start);
            let merged_old_end = (current.old_start + current.old_count - 1)
                .max(next.old_start + next.old_count - 1);
            let merged_new_end = (current.new_start + current.new_count - 1)
                .max(next.new_start + next.new_count - 1);

            // Collect changed indices from both hunks.
            let mut removed_indices = std::collections::HashSet::new();
            let mut added_indices = std::collections::HashSet::new();
            for h in [&current, &next] {
                let mut oi = h.old_start - 1;
                let mut ni = h.new_start - 1;
                for line in &h.lines {
                    match line {
                        DiffLine::Removed(_) => {
                            removed_indices.insert(oi);
                            oi += 1;
                        }
                        DiffLine::Added(_) => {
                            added_indices.insert(ni);
                            ni += 1;
                        }
                        DiffLine::Context(_) => {
                            oi += 1;
                            ni += 1;
                        }
                    }
                }
            }

            // Rebuild the merged line list from the original data.
            let old_lo = merged_old_start - 1;
            let new_lo = merged_new_start - 1;
            let old_hi = merged_old_end - 1;
            let new_hi = merged_new_end - 1;
            let mut lines = Vec::new();
            let mut oi = old_lo;
            let mut ni = new_lo;
            while oi <= old_hi || ni <= new_hi {
                let in_old = oi <= old_hi && oi < old.len();
                let in_new = ni <= new_hi && ni < new.len();
                let old_removed = in_old && removed_indices.contains(&oi);
                let new_added = in_new && added_indices.contains(&ni);

                if old_removed && new_added {
                    lines.push(DiffLine::Removed(old[oi].clone()));
                    lines.push(DiffLine::Added(new[ni].clone()));
                    oi += 1;
                    ni += 1;
                } else if old_removed {
                    lines.push(DiffLine::Removed(old[oi].clone()));
                    oi += 1;
                } else if new_added {
                    lines.push(DiffLine::Added(new[ni].clone()));
                    ni += 1;
                } else if in_old && in_new {
                    lines.push(DiffLine::Context(old[oi].clone()));
                    oi += 1;
                    ni += 1;
                } else if in_old {
                    lines.push(DiffLine::Removed(old[oi].clone()));
                    oi += 1;
                } else if in_new {
                    lines.push(DiffLine::Added(new[ni].clone()));
                    ni += 1;
                } else {
                    break;
                }
            }

            let old_count = lines
                .iter()
                .filter(|l| !matches!(l, DiffLine::Added(_)))
                .count();
            let new_count = lines
                .iter()
                .filter(|l| !matches!(l, DiffLine::Removed(_)))
                .count();

            current = Hunk {
                old_start: merged_old_start,
                old_count,
                new_start: merged_new_start,
                new_count,
                lines,
            };
        } else {
            merged.push(current);
            current = next;
        }
    }
    merged.push(current);
    merged
}

/// Format a hunk header in GNU style: `@@ -a,b +c,d @@`.
///
/// When a count is 1, GNU omits it (`@@ -a +c @@`). When a count is 0, the
/// start is 0 and the count is omitted (`@@ -0 +1 @@`).
pub fn format_hunk_header(h: &Hunk) -> String {
    let old_range = if h.old_count == 0 {
        "0".to_string()
    } else if h.old_count == 1 {
        format!("{}", h.old_start)
    } else {
        format!("{},{}", h.old_start, h.old_count)
    };
    let new_range = if h.new_count == 0 {
        "0".to_string()
    } else if h.new_count == 1 {
        format!("{}", h.new_start)
    } else {
        format!("{},{}", h.new_start, h.new_count)
    };
    format!("@@ -{old_range} +{new_range} @@")
}

/// Format an entire hunk (header plus lines) as a string.
pub fn format_hunk(h: &Hunk) -> String {
    let mut out = String::new();
    out.push_str(&format_hunk_header(h));
    out.push('\n');
    for line in &h.lines {
        match line {
            DiffLine::Context(s) => {
                out.push(' ');
                out.push_str(s);
            }
            DiffLine::Removed(s) => {
                out.push('-');
                out.push_str(s);
            }
            DiffLine::Added(s) => {
                out.push('+');
                out.push_str(s);
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn identical_files_produce_no_hunks() {
        let old = vec_of(&["a", "b", "c"]);
        let new = vec_of(&["a", "b", "c"]);
        let hunks = compute_hunks(&old, &new, 3, false);
        assert!(hunks.is_empty());
    }

    #[test]
    fn single_line_change_produces_one_hunk() {
        let old = vec_of(&["a", "b", "c"]);
        let new = vec_of(&["a", "X", "c"]);
        let hunks = compute_hunks(&old, &new, 0, false);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 2);
        assert_eq!(hunks[0].new_start, 2);
        assert_eq!(hunks[0].old_count, 1);
        assert_eq!(hunks[0].new_count, 1);
        assert_eq!(hunks[0].lines.len(), 2);
        assert_eq!(hunks[0].lines[0], DiffLine::Removed("b".to_string()));
        assert_eq!(hunks[0].lines[1], DiffLine::Added("X".to_string()));
    }

    #[test]
    fn context_lines_are_included() {
        let old = vec_of(&["a", "b", "c", "d", "e"]);
        let new = vec_of(&["a", "b", "X", "d", "e"]);
        let hunks = compute_hunks(&old, &new, 1, false);
        assert_eq!(hunks.len(), 1);
        // Context of 1 around the change at line 3: one line before and one
        // line after, matching GNU diff.
        assert_eq!(hunks[0].old_start, 2);
        assert_eq!(hunks[0].new_start, 2);
        assert_eq!(hunks[0].lines.len(), 4);
        assert_eq!(hunks[0].lines[0], DiffLine::Context("b".to_string()));
        assert_eq!(hunks[0].lines[1], DiffLine::Removed("c".to_string()));
        assert_eq!(hunks[0].lines[2], DiffLine::Added("X".to_string()));
        assert_eq!(hunks[0].lines[3], DiffLine::Context("d".to_string()));
    }

    #[test]
    fn ignore_case_treats_differing_case_as_equal() {
        let old = vec_of(&["Hello", "World"]);
        let new = vec_of(&["hello", "world"]);
        assert!(!compute_hunks(&old, &new, 3, false).is_empty());
        assert!(compute_hunks(&old, &new, 3, true).is_empty());
    }

    #[test]
    fn header_format_matches_gnu() {
        let h = Hunk {
            old_start: 1,
            old_count: 1,
            new_start: 1,
            new_count: 1,
            lines: vec![
                DiffLine::Removed("a".to_string()),
                DiffLine::Added("b".to_string()),
            ],
        };
        assert_eq!(format_hunk_header(&h), "@@ -1 +1 @@");
    }

    #[test]
    fn header_format_with_counts() {
        let h = Hunk {
            old_start: 2,
            old_count: 3,
            new_start: 4,
            new_count: 5,
            lines: vec![],
        };
        assert_eq!(format_hunk_header(&h), "@@ -2,3 +4,5 @@");
    }

    #[test]
    fn full_hunk_formatting() {
        let h = Hunk {
            old_start: 1,
            old_count: 1,
            new_start: 1,
            new_count: 1,
            lines: vec![
                DiffLine::Context("c".to_string()),
                DiffLine::Removed("a".to_string()),
                DiffLine::Added("b".to_string()),
            ],
        };
        let text = format_hunk(&h);
        assert_eq!(text, "@@ -1 +1 @@\n c\n-a\n+b\n");
    }

    #[test]
    fn insertion_at_end() {
        let old = vec_of(&["a", "b"]);
        let new = vec_of(&["a", "b", "c"]);
        let hunks = compute_hunks(&old, &new, 0, false);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].new_start, 3);
        assert_eq!(hunks[0].lines, vec![DiffLine::Added("c".to_string())]);
    }

    #[test]
    fn multiple_hunks_are_split() {
        // Two changes far apart should produce two hunks.
        let old = vec_of(&["a", "b", "c", "d", "e", "f", "g"]);
        let new = vec_of(&["X", "b", "c", "d", "e", "f", "Y"]);
        let hunks = compute_hunks(&old, &new, 0, false);
        assert_eq!(hunks.len(), 2);
    }

    #[test]
    fn adjacent_changes_merge_into_one_hunk() {
        let old = vec_of(&["a", "b", "c"]);
        let new = vec_of(&["X", "Y", "Z"]);
        let hunks = compute_hunks(&old, &new, 0, false);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 6);
    }

    #[test]
    fn trailing_context_line_is_not_marked_changed() {
        // Regression test: with a changed middle line, a trailing common line
        // must be recognized as context, not spuriously marked removed+added.
        let old = vec_of(&["alpha", "CHANGED", "gamma"]);
        let new = vec_of(&["alpha", "changed", "gamma"]);
        let hunks = compute_hunks(&old, &new, 3, false);
        assert_eq!(hunks.len(), 1);
        // The only change is the middle line: 'CHANGED' removed, 'changed'
        // added, with 'alpha' and 'gamma' as context.
        let lines = &hunks[0].lines;
        assert_eq!(lines[0], DiffLine::Context("alpha".to_string()));
        assert_eq!(lines[1], DiffLine::Removed("CHANGED".to_string()));
        assert_eq!(lines[2], DiffLine::Added("changed".to_string()));
        assert_eq!(lines[3], DiffLine::Context("gamma".to_string()));
        assert_eq!(hunks[0].old_count, 3);
        assert_eq!(hunks[0].new_count, 3);
    }

    #[test]
    fn changed_line_in_middle_keeps_common_context() {
        // A changed line between two identical lines should keep both the
        // leading and trailing lines as context (GNU parity).
        let old = vec_of(&["alpha", "CHANGED", "gamma"]);
        let new = vec_of(&["alpha", "changed", "gamma"]);
        let hunks = compute_hunks(&old, &new, 3, false);
        let text = format_hunk(&hunks[0]);
        assert!(text.contains(" alpha\n"));
        assert!(text.contains("-CHANGED\n"));
        assert!(text.contains("+changed\n"));
        assert!(text.contains(" gamma\n"));
        // 'gamma' must not be marked as changed.
        assert!(!text.contains("-gamma\n"));
        assert!(!text.contains("+gamma\n"));
    }
}
