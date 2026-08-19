# ruling

A friendly, dependency-free `diff` clone for the command line.

Compares two files line-by-line and reports whether they are identical,
different, or binary — with GNU-style unified output that matches `diff -u`.

## Usage

```
ruling <file-a> <file-b> [OPTIONS]
```

### Options

| Flag | Description |
|------|-------------|
| `-u`, `--unified N` | Number of context lines (default: 3) |
| `-i`, `--ignore-case` | Compare case-insensitively |
| `-w`, `--ignore-all-space` | Ignore all whitespace when comparing |
| `-q`, `--brief` | Only report whether the files differ |
| `--word-diff` | Show word-level intra-line changes |
| `--json` | Output diff as structured JSON |
| `-h`, `--help` | Print help and exit |
| `-V`, `--version` | Print version and exit |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Files are identical |
| `1` | Files differ (or are binary) |
| `2` | Error (missing file, directory operand, unknown flag) |

## Examples

```bash
# Basic comparison
ruling old.py new.py

# Unified diff with 5 lines of context
ruling -u 5 old.py new.py

# Case-insensitive comparison
ruling -i Config.toml config.toml

# Quick check — just tell me if they differ
ruling -q file1.txt file2.txt

# Ignore whitespace differences
ruling -w old.py new.py

# Word-level diff (highlights what changed within each line)
ruling --word-diff old.py new.py

# JSON output for AI/automation
ruling --json old.py new.py
```

## Features

### Word-level diff

`--word-diff` highlights exactly which words changed within each line, using
`{added}` and `[-removed-]` markers. This makes it easy to see small changes
in long lines.

### JSON output

`--json` outputs the diff as structured JSON — ideal for AI tools, automation,
and programmatic consumption. The output includes file metadata, per-hunk
details with line numbers, and a summary with insertion/deletion counts.

```json
{
  "meta": { "old_file": "a.py", "new_file": "b.py", "identical": false },
  "hunks": [
    {
      "old_start": 5, "old_count": 3, "new_start": 5, "new_count": 4,
      "context": "fn process_order",
      "lines": [
        { "type": "context", "content": " let x = 1;", "old_line": 5, "new_line": 5 },
        { "type": "removed", "content": " let x = 2;", "old_line": 6 },
        { "type": "added", "content": " let x = 3;", "new_line": 6 }
      ]
    }
  ],
  "summary": { "files_changed": 1, "insertions": 1, "deletions": 1 }
}
```

### Whitespace filtering

`-w` / `--ignore-all-space` collapses all whitespace runs to a single space
before comparing, so formatter-induced diffs don't clutter the output.

### Function-aware headers

Hunk headers include the containing function/struct/class name when available:
`@@ -5,3 +5,4 @@ fn process_order()`. Supports Rust, Python, JavaScript,
TypeScript, Go, and C/C++.

## How it works

ruling reads both files line-by-line via `BufRead`, so multi-gigabyte files
work without loading them into memory. It computes the longest common
subsequence (LCS) to identify changes, then emits GNU-style unified hunks
with `@@ -a,b +c,d @@` headers and configurable context lines.

Binary files are detected by checking for null bytes and non-UTF-8 content
in a bounded sample at the start of each file. When binary content is
detected, ruling prints `Binary files <a> and <b> differ` and exits 1.

Adjacent hunks whose context regions overlap are automatically merged to
match GNU diff output.

## Installation

```bash
cargo install ruling
```

Or build from source:

```bash
git clone https://github.com/NefaroXX/ruling
cd ruling
cargo build --release
```

## Requirements

- Rust 1.70 or later (for `std::io::IsTerminal`)
- No external dependencies — the standard library only

## License

MIT — see [LICENSE](LICENSE) for details.
