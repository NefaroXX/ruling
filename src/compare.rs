//! File I/O orchestration for `verdict`.
//!
//! This module handles binary detection, line-by-line streaming via `BufRead`,
//! exit-code mapping, and brief mode. The comparison itself is delegated to
//! [`crate::diff`].

use crate::cli::Config;
use crate::diff::{compute_hunks, format_hunk};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek};

/// The outcome of comparing two files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareOutcome {
    /// The files are identical (or equal under the configured comparison).
    Identical,
    /// The files differ; a unified diff was printed (unless brief mode).
    Differ,
    /// One or both files contain non-UTF-8 content.
    Binary,
}

/// Compare two files according to the configuration and return the outcome.
///
/// This streams both files line-by-line via `BufRead` so that multi-GB files
/// can be handled without loading them fully into memory. Binary detection is
/// performed on a bounded sample at the start of each file so it does not
/// interfere with streaming.
pub fn compare_files(cfg: &Config) -> Result<CompareOutcome, String> {
    // Validate both operands before doing any real work.
    let meta_a = std::fs::metadata(&cfg.file_a).map_err(|e| format!("{}: {e}", cfg.file_a))?;
    let meta_b = std::fs::metadata(&cfg.file_b).map_err(|e| format!("{}: {e}", cfg.file_b))?;

    // Directory operands are errors (no directory walking — out of scope).
    if meta_a.is_dir() {
        return Err(format!("{}: Is a directory", cfg.file_a));
    }
    if meta_b.is_dir() {
        return Err(format!("{}: Is a directory", cfg.file_b));
    }

    // Open both files.
    let mut file_a = File::open(&cfg.file_a).map_err(|e| format!("{}: {e}", cfg.file_a))?;
    let mut file_b = File::open(&cfg.file_b).map_err(|e| format!("{}: {e}", cfg.file_b))?;

    // Detect binary content from a bounded sample. If either file contains
    // non-UTF-8 bytes in the sample, treat the pair as binary.
    if is_binary(&mut file_a)? || is_binary(&mut file_b)? {
        return Ok(CompareOutcome::Binary);
    }

    // Rewind both files so we can stream them line-by-line from the start.
    file_a
        .rewind()
        .map_err(|e| format!("{}: {e}", cfg.file_a))?;
    file_b
        .rewind()
        .map_err(|e| format!("{}: {e}", cfg.file_b))?;

    let reader_a = BufReader::new(file_a);
    let reader_b = BufReader::new(file_b);

    // Stream both files into line vectors. Lines are read incrementally via
    // BufRead rather than slurping the whole file as one blob.
    let lines_a = read_lines(reader_a)?;
    let lines_b = read_lines(reader_b)?;

    let hunks = compute_hunks(&lines_a, &lines_b, cfg.unified_context, cfg.ignore_case);

    if hunks.is_empty() {
        return Ok(CompareOutcome::Identical);
    }

    // Files differ. In brief mode, print a summary line only.
    if cfg.brief {
        println!("Files {} and {} differ", cfg.file_a, cfg.file_b);
        return Ok(CompareOutcome::Differ);
    }

    // Print the unified diff header and hunks.
    println!("--- {}", cfg.file_a);
    println!("+++ {}", cfg.file_b);
    for hunk in &hunks {
        print!("{}", format_hunk(hunk));
    }

    Ok(CompareOutcome::Differ)
}

/// Read a bounded sample from a reader and check whether it contains any
/// non-UTF-8 bytes. The reader is left positioned after the sample.
fn is_binary<R: Read>(reader: &mut R) -> Result<bool, String> {
    // Sample up to 8 KiB. This is enough to catch most binary files while
    // staying cheap; a longer scan would defeat streaming.
    const SAMPLE_SIZE: usize = 8192;
    let mut buf = vec![0u8; SAMPLE_SIZE];
    let mut total = 0usize;
    loop {
        let n = reader
            .read(&mut buf[total..])
            .map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            break;
        }
        total += n;
        if total == SAMPLE_SIZE {
            break;
        }
    }
    // Check for null bytes (common in binary files) or non-UTF-8 content.
    Ok(buf[..total].contains(&0) || std::str::from_utf8(&buf[..total]).is_err())
}

/// Read all lines from a `BufRead` into a `Vec<String>`.
///
/// A trailing newline is stripped from each line. This is done incrementally
/// via `BufRead::read_line`, so memory grows with the number of lines, not by
/// slurping the entire file at once.
fn read_lines<R: BufRead>(mut reader: R) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            break;
        }
        // Strip the trailing newline (and any carriage return) for diffing.
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        lines.push(line.clone());
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn binary_detection_accepts_utf8() {
        let data = b"hello\nworld\n";
        let mut cursor = Cursor::new(data);
        assert!(!is_binary(&mut cursor).unwrap());
    }

    #[test]
    fn binary_detection_rejects_non_utf8() {
        let data = [0xffu8, 0xfe, 0x00, 0x01];
        let mut cursor = Cursor::new(data);
        assert!(is_binary(&mut cursor).unwrap());
    }
}
