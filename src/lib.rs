//! `verdict` is a minimal, dependency-free CLI tool that behaves like a
//! friendly `diff` clone. It compares two files line-by-line and reports
//! whether they are identical, different, or binary.
//!
//! The crate is organized into three modules:
//! - [`cli`]: hand-rolled command-line argument parsing.
//! - [`diff`]: the core line-diff engine producing unified hunks.
//! - [`compare`]: file I/O orchestration, binary detection, and exit-code
//!   mapping.

pub mod cli;
pub mod compare;
pub mod diff;
