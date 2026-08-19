//! Entry point for the `verdict` CLI.
//!
//! Handles argument parsing, orchestrates the comparison, and maps the result
//! to the appropriate process exit code.

use std::process::ExitCode;
use verdict::cli::{self, ParseError};
use verdict::compare::{compare_files, CompareOutcome};

/// The process exit code for `verdict`.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Handle --help and --version early; both exit with code 0.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        cli::print_usage(false);
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        cli::print_version();
        return ExitCode::SUCCESS;
    }

    let cfg = match cli::parse_args(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            // Any parse error is an error condition: print usage to stderr and
            // exit with code 2.
            eprintln!("verdict: {}", describe_parse_error(&err));
            cli::print_usage(true);
            return ExitCode::from(2);
        }
    };

    match compare_files(&cfg) {
        Ok(CompareOutcome::Identical) => ExitCode::SUCCESS,
        Ok(CompareOutcome::Differ) => ExitCode::from(1),
        Ok(CompareOutcome::Binary) => {
            println!("Binary files {} and {} differ", cfg.file_a, cfg.file_b);
            ExitCode::from(1)
        }
        Err(msg) => {
            eprintln!("verdict: {msg}");
            ExitCode::from(2)
        }
    }
}

/// Produce a human-readable message for a [`ParseError`].
fn describe_parse_error(err: &ParseError) -> String {
    match err {
        ParseError::UnknownFlag(flag) => format!("unknown option '{flag}'"),
        ParseError::MissingArg => "missing file operand".to_string(),
        ParseError::ExtraArg => "too many file operands".to_string(),
        ParseError::BadUnified(v) => {
            format!("invalid --unified value '{v}' (expected a non-negative integer)")
        }
    }
}
