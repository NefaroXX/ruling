//! Hand-rolled command-line argument parsing for `ruling`.
//!
//! The CLI surface is `ruling <file-a> <file-b> [OPTIONS]`. We deliberately
//! avoid clap or any argument-parsing crate to keep the crate dependency-free.

/// The parsed command-line configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the first file (the "old" file).
    pub file_a: String,
    /// Path to the second file (the "new" file).
    pub file_b: String,
    /// Number of context lines to include around hunks (default 3).
    pub unified_context: usize,
    /// Whether to compare case-insensitively (`-i`).
    pub ignore_case: bool,
    /// Whether to run in brief mode (`-q`), printing only a summary line.
    pub brief: bool,
}

/// Errors produced while parsing command-line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// An unrecognized flag was supplied.
    UnknownFlag(String),
    /// A required argument (a file operand) was missing.
    MissingArg,
    /// More than two file operands were supplied.
    ExtraArg,
    /// The value for `--unified` was missing or not a non-negative integer.
    BadUnified(String),
}

/// Parse the command-line arguments (excluding the program name) into a
/// [`Config`].
///
/// The two positional operands are the file paths. Flags may appear in any
/// position:
/// - `-u N` / `--unified N`: set the number of context lines (default 3).
/// - `-i` / `--ignore-case`: compare case-insensitively.
/// - `-q` / `--brief`: only report whether the files differ.
/// - `--help` and `--version` are handled separately by `main` before this
///   parser is asked for a full config; they are still accepted here so that
///   tests can exercise them through the same path.
pub fn parse_args(args: &[String]) -> Result<Config, ParseError> {
    let mut file_a: Option<String> = None;
    let mut file_b: Option<String> = None;
    let mut unified_context: usize = 3;
    let mut ignore_case = false;
    let mut brief = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-i" | "--ignore-case" => {
                ignore_case = true;
            }
            "-q" | "--brief" => {
                brief = true;
            }
            "-u" | "--unified" => {
                // The next argument is the context count.
                i += 1;
                if i >= args.len() {
                    return Err(ParseError::BadUnified(String::new()));
                }
                unified_context = parse_context(&args[i])?;
            }
            _ if arg.starts_with("--unified=") => {
                let value = &arg["--unified=".len()..];
                unified_context = parse_context(value)?;
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                // Unknown flag.
                return Err(ParseError::UnknownFlag(arg.clone()));
            }
            _ => {
                // A positional operand.
                if file_a.is_none() {
                    file_a = Some(arg.clone());
                } else if file_b.is_none() {
                    file_b = Some(arg.clone());
                } else {
                    return Err(ParseError::ExtraArg);
                }
            }
        }
        i += 1;
    }

    // Both file operands are required.
    let file_a = file_a.ok_or(ParseError::MissingArg)?;
    let file_b = file_b.ok_or(ParseError::MissingArg)?;

    Ok(Config {
        file_a,
        file_b,
        unified_context,
        ignore_case,
        brief,
    })
}

/// Parse a non-negative integer used as the unified context count.
fn parse_context(value: &str) -> Result<usize, ParseError> {
    value
        .parse::<usize>()
        .map_err(|_| ParseError::BadUnified(value.to_string()))
}

/// Print the usage/help text.
///
/// When `stderr` is true the text is written to stderr (used on errors);
/// otherwise it is written to stdout (used for `--help`).
pub fn print_usage(stderr: bool) {
    let text = "\
ruling 0.1.0 — a friendly diff clone

USAGE:
    ruling <file-a> <file-b> [OPTIONS]

ARGS:
    <file-a>    The first file to compare
    <file-b>    The second file to compare

OPTIONS:
    -u, --unified N    Number of context lines (default: 3)
    -i, --ignore-case  Ignore case when comparing lines
    -q, --brief        Only report whether the files differ
    -h, --help         Print this help and exit
    -V, --version      Print version information and exit

EXIT CODES:
    0    Files are identical
    1    Files differ (or are binary)
    2    An error occurred (missing file, directory, unknown flag)
";
    if stderr {
        eprint!("{text}");
    } else {
        print!("{text}");
    }
}

/// Print the version string to stdout.
pub fn print_version() {
    println!("ruling {}", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_two_positional_files() {
        let cfg = parse_args(&args(&["a.txt", "b.txt"])).unwrap();
        assert_eq!(cfg.file_a, "a.txt");
        assert_eq!(cfg.file_b, "b.txt");
        assert_eq!(cfg.unified_context, 3);
        assert!(!cfg.ignore_case);
        assert!(!cfg.brief);
    }

    #[test]
    fn parses_unified_flag_with_value() {
        let cfg = parse_args(&args(&["-u", "5", "a.txt", "b.txt"])).unwrap();
        assert_eq!(cfg.unified_context, 5);
    }

    #[test]
    fn parses_unified_equals_form() {
        let cfg = parse_args(&args(&["--unified=0", "a.txt", "b.txt"])).unwrap();
        assert_eq!(cfg.unified_context, 0);
    }

    #[test]
    fn parses_ignore_case_and_brief() {
        let cfg = parse_args(&args(&["-i", "-q", "a.txt", "b.txt"])).unwrap();
        assert!(cfg.ignore_case);
        assert!(cfg.brief);
    }

    #[test]
    fn rejects_unknown_flag() {
        let err = parse_args(&args(&["--bogus", "a.txt", "b.txt"])).unwrap_err();
        assert_eq!(err, ParseError::UnknownFlag("--bogus".to_string()));
    }

    #[test]
    fn rejects_missing_argument() {
        let err = parse_args(&args(&["only-one.txt"])).unwrap_err();
        assert_eq!(err, ParseError::MissingArg);
    }

    #[test]
    fn rejects_extra_argument() {
        let err = parse_args(&args(&["a.txt", "b.txt", "c.txt"])).unwrap_err();
        assert_eq!(err, ParseError::ExtraArg);
    }

    #[test]
    fn rejects_bad_unified_value() {
        let err = parse_args(&args(&["-u", "abc", "a.txt", "b.txt"])).unwrap_err();
        assert_eq!(err, ParseError::BadUnified("abc".to_string()));
    }
}
