# verdict

A friendly, dependency-free `diff` clone for the command line.

Compares two files line-by-line and reports whether they are identical,
different, or binary — with GNU-style unified output that matches `diff -u`.

## Usage

```
verdict <file-a> <file-b> [OPTIONS]
```

### Options

| Flag | Description |
|------|-------------|
| `-u`, `--unified N` | Number of context lines (default: 3) |
| `-i`, `--ignore-case` | Compare case-insensitively |
| `-q`, `--brief` | Only report whether the files differ |
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
verdict old.py new.py

# Unified diff with 5 lines of context
verdict -u 5 old.py new.py

# Case-insensitive comparison
verdict -i Config.toml config.toml

# Quick check — just tell me if they differ
verdict -q file1.txt file2.txt
```

## How it works

verdict reads both files line-by-line via `BufRead`, so multi-gigabyte files
work without loading them into memory. It computes the longest common
subsequence (LCS) to identify changes, then emits GNU-style unified hunks
with `@@ -a,b +c,d @@` headers and configurable context lines.

Binary files are detected by checking for null bytes and non-UTF-8 content
in a bounded sample at the start of each file. When binary content is
detected, verdict prints `Binary files <a> and <b> differ` and exits 1.

Adjacent hunks whose context regions overlap are automatically merged to
match GNU diff output.

## Installation

```bash
cargo install verdict
```

Or build from source:

```bash
git clone https://github.com/NefaroXX/verdict
cd verdict
cargo build --release
```

## Requirements

- Rust 1.70 or later (for `std::io::IsTerminal`)
- No external dependencies — the standard library only

## License

MIT — see [LICENSE](LICENSE) for details.
