//! Core line-diff engine for `ruling`.
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

/// A word-level segment within a changed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordDiff {
    /// An unchanged word.
    Context(String),
    /// A word removed from the old file.
    Removed(String),
    /// A word added in the new file.
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

/// Options that control how hunks are computed and formatted.
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Number of context lines around changes (default 3).
    pub context: usize,
    /// Compare lines case-insensitively.
    pub ignore_case: bool,
    /// Ignore all whitespace when comparing lines.
    pub ignore_all_space: bool,
    /// Show word-level intra-line highlighting.
    pub word_diff: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            context: 3,
            ignore_case: false,
            ignore_all_space: false,
            word_diff: false,
        }
    }
}

/// Compute a list of unified hunks describing the differences between `old`
/// and `new`.
///
/// This is the primary entry point for backward compatibility. For full
/// control over options, use [`compute_hunks_with_options`].
pub fn compute_hunks(
    old: &[String],
    new: &[String],
    context: usize,
    ignore_case: bool,
) -> Vec<Hunk> {
    compute_hunks_with_options(
        old,
        new,
        &DiffOptions {
            context,
            ignore_case,
            ..DiffOptions::default()
        },
    )
}

/// Compute hunks with full control over comparison and formatting options.
pub fn compute_hunks_with_options(old: &[String], new: &[String], opts: &DiffOptions) -> Vec<Hunk> {
    let eq = make_line_comparator(opts);

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
                if cursor > last_change + opts.context {
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
        let old_lo = change_pos.saturating_sub(opts.context);
        let new_lo = change_pos.saturating_sub(opts.context);
        let old_hi = extent.saturating_add(opts.context).min(n.saturating_sub(1));
        let new_hi = extent.saturating_add(opts.context).min(m.saturating_sub(1));

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

/// Build a line-equality comparator based on the diff options.
fn make_line_comparator(opts: &DiffOptions) -> impl Fn(&str, &str) -> bool {
    let ignore_case = opts.ignore_case;
    let ignore_all_space = opts.ignore_all_space;
    move |a: &str, b: &str| {
        let (a_norm, b_norm) = if ignore_all_space {
            (collapse_whitespace(a), collapse_whitespace(b))
        } else {
            (a.to_owned(), b.to_owned())
        };
        if ignore_case {
            a_norm.eq_ignore_ascii_case(&b_norm)
        } else {
            a_norm == b_norm
        }
    }
}

/// Collapse all whitespace runs to a single space, for whitespace-ignoring
/// comparison.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_space = false;
    for ch in s.chars() {
        if ch.is_ascii_whitespace() {
            if !in_space {
                result.push(' ');
                in_space = true;
            }
        } else {
            result.push(ch);
            in_space = false;
        }
    }
    result
}

/// Compute word-level diff between two changed lines.
///
/// Splits each line into whitespace-delimited words, then computes the LCS
/// of the word arrays to identify which words changed.
pub fn compute_word_diff(old_line: &str, new_line: &str) -> Vec<WordDiff> {
    let old_words: Vec<&str> = old_line.split_whitespace().collect();
    let new_words: Vec<&str> = new_line.split_whitespace().collect();
    let n = old_words.len();
    let m = new_words.len();

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if old_words[i] == new_words[j] {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else {
                dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut result = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if old_words[i] == new_words[j] {
            result.push(WordDiff::Context(old_words[i].to_string()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            result.push(WordDiff::Removed(old_words[i].to_string()));
            i += 1;
        } else {
            result.push(WordDiff::Added(new_words[j].to_string()));
            j += 1;
        }
    }
    while i < n {
        result.push(WordDiff::Removed(old_words[i].to_string()));
        i += 1;
    }
    while j < m {
        result.push(WordDiff::Added(new_words[j].to_string()));
        j += 1;
    }

    result
}

/// Format word diff segments into a string with `{added}` and `[-removed-]`
/// markers.
pub fn format_word_diff(segments: &[WordDiff]) -> String {
    let mut out = String::new();
    for seg in segments {
        match seg {
            WordDiff::Context(w) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(w);
            }
            WordDiff::Removed(w) => {
                if !out.is_empty() && !out.ends_with('[') {
                    out.push(' ');
                }
                out.push_str("[-");
                out.push_str(w);
                out.push_str("-]");
            }
            WordDiff::Added(w) => {
                if !out.is_empty() && !out.ends_with('{') {
                    out.push(' ');
                }
                out.push('{');
                out.push_str(w);
                out.push('}');
            }
        }
    }
    out
}

/// Detect the containing function/struct/class name by scanning backward
/// from a given position in the file lines.
pub fn detect_context_function(lines: &[String], start_idx: usize) -> Option<String> {
    let search_start = start_idx.saturating_sub(100);
    for i in (search_start..start_idx).rev() {
        if let Some(line) = lines.get(i) {
            let trimmed = line.trim();
            if is_definition_line(trimmed) {
                let display = if trimmed.len() > 60 {
                    format!("{}...", &trimmed[..57])
                } else {
                    trimmed.to_string()
                };
                return Some(display);
            }
        }
    }
    None
}

/// Check if a line looks like a function/struct/class/impl definition.
fn is_definition_line(line: &str) -> bool {
    if line.is_empty() || line.starts_with("//") || line.starts_with('#') || line.starts_with("/*")
    {
        return false;
    }

    // Rust
    if line.starts_with("fn ")
        || line.starts_with("pub fn ")
        || line.starts_with("pub(crate) fn ")
        || line.starts_with("pub(super) fn ")
        || line.starts_with("async fn ")
        || line.starts_with("pub async fn ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
        || line.starts_with("impl ")
        || line.starts_with("trait ")
        || line.starts_with("type ")
        || line.starts_with("const ")
        || line.starts_with("pub const ")
        || line.starts_with("static ")
        || line.starts_with("pub static ")
        || line.starts_with("mod ")
        || line.starts_with("pub mod ")
    {
        return true;
    }

    // Python
    if line.starts_with("def ") || line.starts_with("class ") || line.starts_with("async def ") {
        return true;
    }

    // JavaScript/TypeScript
    if line.starts_with("function ")
        || line.starts_with("export function ")
        || line.starts_with("export default function ")
        || line.starts_with("class ")
        || line.starts_with("export class ")
    {
        return true;
    }

    // Go
    if line.starts_with("func ") || line.starts_with("func (") {
        return true;
    }

    // C/C++ heuristic: line with parens ending in `{`
    if line.contains('(')
        && !line.starts_with("if ")
        && !line.starts_with("for ")
        && !line.starts_with("while ")
        && !line.starts_with("match ")
        && !line.starts_with("switch ")
        && !line.starts_with("return ")
        && !line.starts_with("else")
        && (line.ends_with('{') || line.ends_with(") {") || line.ends_with(");"))
    {
        return true;
    }

    false
}

/// Escape a string for JSON output.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Format a list of hunks as a JSON string.
pub fn format_json(
    old_file: &str,
    new_file: &str,
    identical: bool,
    hunks: &[Hunk],
    old_lines: &[String],
) -> String {
    let mut out = String::from("{\n");

    out.push_str("  \"meta\": {\n");
    out.push_str(&format!(
        "    \"old_file\": \"{}\",\n",
        json_escape(old_file)
    ));
    out.push_str(&format!(
        "    \"new_file\": \"{}\",\n",
        json_escape(new_file)
    ));
    out.push_str(&format!("    \"identical\": {}\n", identical));
    out.push_str("  },\n");

    out.push_str("  \"hunks\": [\n");
    for (idx, hunk) in hunks.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"old_start\": {},\n", hunk.old_start));
        out.push_str(&format!("      \"old_count\": {},\n", hunk.old_count));
        out.push_str(&format!("      \"new_start\": {},\n", hunk.new_start));
        out.push_str(&format!("      \"new_count\": {},\n", hunk.new_count));

        let ctx = detect_context_function(old_lines, hunk.old_start.saturating_sub(1));
        if let Some(ref c) = ctx {
            out.push_str(&format!("      \"context\": \"{}\",\n", json_escape(c)));
        }

        out.push_str("      \"lines\": [\n");
        let mut oi = hunk.old_start.saturating_sub(1);
        let mut ni = hunk.new_start.saturating_sub(1);
        for (line_idx, line) in hunk.lines.iter().enumerate() {
            let comma = if line_idx + 1 < hunk.lines.len() {
                ","
            } else {
                ""
            };
            match line {
                DiffLine::Context(s) => {
                    out.push_str(&format!(
                        "        {{ \"type\": \"context\", \"content\": \"{}\", \"old_line\": {}, \"new_line\": {} }}{}\n",
                        json_escape(s), oi + 1, ni + 1, comma
                    ));
                    oi += 1;
                    ni += 1;
                }
                DiffLine::Removed(s) => {
                    out.push_str(&format!(
                        "        {{ \"type\": \"removed\", \"content\": \"{}\", \"old_line\": {} }}{}\n",
                        json_escape(s), oi + 1, comma
                    ));
                    oi += 1;
                }
                DiffLine::Added(s) => {
                    out.push_str(&format!(
                        "        {{ \"type\": \"added\", \"content\": \"{}\", \"new_line\": {} }}{}\n",
                        json_escape(s), ni + 1, comma
                    ));
                    ni += 1;
                }
            }
        }
        out.push_str("      ]\n");

        let comma = if idx + 1 < hunks.len() { "," } else { "" };
        out.push_str(&format!("    }}{}\n", comma));
    }
    out.push_str("  ],\n");

    let insertions: usize = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| matches!(l, DiffLine::Added(_)))
        .count();
    let deletions: usize = hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| matches!(l, DiffLine::Removed(_)))
        .count();
    let files_changed = if identical { 0 } else { 1 };

    out.push_str("  \"summary\": {\n");
    out.push_str(&format!("    \"files_changed\": {},\n", files_changed));
    out.push_str(&format!("    \"insertions\": {},\n", insertions));
    out.push_str(&format!("    \"deletions\": {}\n", deletions));
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

/// Format a hunk header in GNU style: `@@ -a,b +c,d @@`.
///
/// When a count is 1, GNU omits it. When a count is 0, the start is 0
/// and the count is omitted. An optional context string (e.g. function
/// name) is appended after `@@`.
pub fn format_hunk_header(h: &Hunk, context: Option<&str>) -> String {
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
    match context {
        Some(ctx) => format!("@@ -{old_range} +{new_range} @@ {ctx}"),
        None => format!("@@ -{old_range} +{new_range} @@"),
    }
}

/// Format an entire hunk with options (word-diff, context headers).
pub fn format_hunk_with_options(h: &Hunk, opts: &DiffOptions, old_lines: &[String]) -> String {
    let mut out = String::new();

    let ctx = if opts.context > 0 {
        detect_context_function(old_lines, h.old_start.saturating_sub(1))
    } else {
        None
    };
    out.push_str(&format_hunk_header(h, ctx.as_deref()));
    out.push('\n');

    if opts.word_diff {
        format_hunk_lines_word_diff(h, &mut out);
    } else {
        format_hunk_lines_plain(h, &mut out);
    }

    out
}

/// Format hunk lines with plain markers (no word diff).
fn format_hunk_lines_plain(h: &Hunk, out: &mut String) {
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
}

/// Format hunk lines with word-level diff markers for changed line pairs.
fn format_hunk_lines_word_diff(h: &Hunk, out: &mut String) {
    let mut i = 0;
    while i < h.lines.len() {
        match &h.lines[i] {
            DiffLine::Context(s) => {
                out.push(' ');
                out.push_str(s);
                out.push('\n');
                i += 1;
            }
            DiffLine::Removed(old_line) => {
                if let Some(DiffLine::Added(new_line)) = h.lines.get(i + 1) {
                    let segments = compute_word_diff(old_line, new_line);
                    let formatted = format_word_diff(&segments);
                    out.push_str("-[-");
                    out.push_str(old_line);
                    out.push_str("-]\n");
                    out.push_str("+{");
                    out.push_str(new_line);
                    out.push_str("}\n");
                    out.push(' ');
                    out.push_str(&formatted);
                    out.push('\n');
                    i += 2;
                } else {
                    out.push('-');
                    out.push_str(old_line);
                    out.push('\n');
                    i += 1;
                }
            }
            DiffLine::Added(s) => {
                out.push('+');
                out.push_str(s);
                out.push('\n');
                i += 1;
            }
        }
    }
}

/// Format an entire hunk (header plus lines) as a string.
pub fn format_hunk(h: &Hunk) -> String {
    let mut out = String::new();
    out.push_str(&format_hunk_header(h, None));
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
        assert_eq!(format_hunk_header(&h, None), "@@ -1 +1 @@");
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
        assert_eq!(format_hunk_header(&h, None), "@@ -2,3 +4,5 @@");
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

    // --- New feature tests ---

    #[test]
    fn ignore_all_space_treats_whitespace_differences_as_equal() {
        let old = vec_of(&["let  x  =  1;", "let y = 2;"]);
        let new = vec_of(&["let x = 1;", "let y = 2;"]);
        let opts = DiffOptions {
            ignore_all_space: true,
            ..DiffOptions::default()
        };
        let hunks = compute_hunks_with_options(&old, &new, &opts);
        assert!(
            hunks.is_empty(),
            "whitespace-only differences should be ignored"
        );
    }

    #[test]
    fn ignore_all_space_still_detects_real_changes() {
        let old = vec_of(&["let  x  =  1;", "let y = 2;"]);
        let new = vec_of(&["let x = 99;", "let y = 2;"]);
        let opts = DiffOptions {
            ignore_all_space: true,
            ..DiffOptions::default()
        };
        let hunks = compute_hunks_with_options(&old, &new, &opts);
        assert!(!hunks.is_empty(), "real content changes should be detected");
    }

    #[test]
    fn collapse_whitespace_basic() {
        assert_eq!(collapse_whitespace("  hello   world  "), " hello world ");
        assert_eq!(collapse_whitespace("no_change"), "no_change");
        assert_eq!(collapse_whitespace("  a\tb\n"), " a b ");
    }

    #[test]
    fn word_diff_detects_changed_word() {
        let segments = compute_word_diff("the quick brown fox", "the quick red fox");
        // LCS: the, quick, fox (3 common). brown removed, red added.
        // Result: Context(the), Context(quick), Removed(brown), Added(red), Context(fox)
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0], WordDiff::Context("the".to_string()));
        assert_eq!(segments[1], WordDiff::Context("quick".to_string()));
        assert_eq!(segments[2], WordDiff::Removed("brown".to_string()));
        assert_eq!(segments[3], WordDiff::Added("red".to_string()));
        assert_eq!(segments[4], WordDiff::Context("fox".to_string()));
    }

    #[test]
    fn word_diff_all_changed() {
        let segments = compute_word_diff("aaa bbb", "ccc ddd");
        assert_eq!(segments.len(), 4);
        assert!(segments
            .iter()
            .all(|s| matches!(s, WordDiff::Removed(_) | WordDiff::Added(_))));
    }

    #[test]
    fn format_word_diff_basic() {
        let segments = vec![
            WordDiff::Context("the".to_string()),
            WordDiff::Removed("brown".to_string()),
            WordDiff::Added("red".to_string()),
            WordDiff::Context("fox".to_string()),
        ];
        let formatted = format_word_diff(&segments);
        assert!(formatted.contains("the"));
        assert!(formatted.contains("[-brown-]"));
        assert!(formatted.contains("{red}"));
        assert!(formatted.contains("fox"));
    }

    #[test]
    fn json_escape_quotes_and_backslashes() {
        assert_eq!(json_escape(r#"he said "hello""#), r#"he said \"hello\""#);
        assert_eq!(json_escape(r#"path\to\file"#), r#"path\\to\\file"#);
        assert_eq!(json_escape("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn json_output_structure() {
        let old = vec_of(&["a", "b"]);
        let new = vec_of(&["a", "X"]);
        let hunks = compute_hunks(&old, &new, 0, false);
        let json = format_json("old.txt", "new.txt", false, &hunks, &old);
        assert!(json.contains("\"old_file\": \"old.txt\""));
        assert!(json.contains("\"new_file\": \"new.txt\""));
        assert!(json.contains("\"identical\": false"));
        assert!(json.contains("\"insertions\":"));
        assert!(json.contains("\"deletions\":"));
    }

    #[test]
    fn detect_context_function_rust() {
        let lines = vec_of(&[
            "use std::io;",
            "",
            "fn main() {",
            "    let x = 1;",
            "}",
            "",
            "fn process_order(order: &Order) -> Result<()> {",
            "    // do stuff",
            "    let y = 2;",
            "}",
        ]);
        let ctx = detect_context_function(&lines, 8);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("fn process_order"));
    }

    #[test]
    fn detect_context_function_python() {
        let lines = vec_of(&["class Foo:", "    def bar(self):", "        pass"]);
        let ctx = detect_context_function(&lines, 2);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("def bar"));
    }

    #[test]
    fn header_format_with_context() {
        let h = Hunk {
            old_start: 5,
            old_count: 3,
            new_start: 5,
            new_count: 4,
            lines: vec![],
        };
        assert_eq!(
            format_hunk_header(&h, Some("fn process_order")),
            "@@ -5,3 +5,4 @@ fn process_order"
        );
    }
}
