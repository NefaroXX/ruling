//! End-to-end integration tests exercising the `ruling` CLI surface.
//!
//! These tests build and run the actual binary via `std::process::Command` to
//! verify the real CLI behaviour including exit codes and output.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Build the path to the compiled `ruling` binary.
fn binary() -> PathBuf {
    // Integration tests run with CARGO_BIN_EXE_<name> set by Cargo.
    PathBuf::from(env!("CARGO_BIN_EXE_ruling"))
}

/// Run `ruling` with the given arguments and a temporary working directory.
fn run(args: &[&str], cwd: &PathBuf) -> Output {
    Command::new(binary())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run ruling")
}

/// Create a temporary directory for a test, returning its path.
fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ruling_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

fn write(path: &PathBuf, contents: &str) {
    fs::write(path, contents).expect("failed to write file");
}

#[test]
fn identical_files_exit_zero_no_output() {
    let dir = tempdir("identical");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "line1\nline2\nline3\n");
    write(&b, "line1\nline2\nline3\n");

    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn different_files_exit_one() {
    let dir = tempdir("different");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "line1\nline2\nline3\n");
    write(&b, "line1\nchanged\nline3\n");

    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("@@"));
}

#[test]
fn missing_file_exits_two() {
    let dir = tempdir("missing");
    let a = dir.join("exists.txt");
    let b = dir.join("missing.txt");
    write(&a, "content\n");

    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
}

#[test]
fn directory_operand_exits_two() {
    let dir = tempdir("dir_operand");
    let a = dir.join("a.txt");
    let sub = dir.join("subdir");
    fs::create_dir_all(&sub).expect("failed to create subdir");
    write(&a, "content\n");

    let out = run(&[a.to_str().unwrap(), sub.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
}

#[test]
fn unknown_flag_exits_two() {
    let dir = tempdir("unknown_flag");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "a\n");
    write(&b, "b\n");

    let out = run(&["--bogus", a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
}

#[test]
fn help_exits_zero() {
    let dir = tempdir("help");
    let out = run(&["--help"], &dir);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USAGE"));
}

#[test]
fn version_exits_zero() {
    let dir = tempdir("version");
    let out = run(&["--version"], &dir);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ruling"));
}

#[test]
fn brief_mode_prints_summary() {
    let dir = tempdir("brief");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "a\n");
    write(&b, "b\n");

    let out = run(&["-q", a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("differ"));
    assert!(!stdout.contains("@@"));
}

#[test]
fn ignore_case_treats_as_identical() {
    let dir = tempdir("ignore_case");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "Hello World\n");
    write(&b, "hello world\n");

    // Without -i they differ.
    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(1));

    // With -i they are identical.
    let out = run(&["-i", a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn binary_files_are_detected() {
    let dir = tempdir("binary");
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
    fs::write(&a, [0xffu8, 0xfe, 0x00, 0x01]).expect("write binary");
    fs::write(&b, [0x00u8, 0x01, 0x02, 0x03]).expect("write binary");

    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Binary files"));
}

#[test]
fn unified_context_flag_controls_context() {
    let dir = tempdir("unified");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    write(&a, "1\n2\n3\n4\n5\n6\n7\n");
    write(&b, "1\n2\n3\nX\n5\n6\n7\n");

    // With -u 0 there should be no context lines.
    let out = run(&["-u", "0", a.to_str().unwrap(), b.to_str().unwrap()], &dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1));
    // Only the changed line and its +/- markers.
    assert!(stdout.contains("-4\n"));
    assert!(stdout.contains("+X\n"));
}
