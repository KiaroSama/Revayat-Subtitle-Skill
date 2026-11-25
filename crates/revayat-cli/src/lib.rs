//! CLI entry points for Revayat Subtitle.

pub mod cli;
pub mod commands;
pub mod install;
pub mod sync_bridge;

use clap::Parser;
use revayat_core::runtime::{LogLevel, OperationalLog, diagnostic::redact};
use std::ffi::OsString;

/// Run the command using the process's argument list.
pub fn run() -> i32 {
    run_from(std::env::args_os())
}

/// Run a command from argv, including its executable name, and return its exit code.
pub fn run_from(args: impl IntoIterator<Item = impl Into<OsString> + Clone>) -> i32 {
    let args = match cli::Cli::try_parse_from(args) {
        Ok(args) => args,
        Err(error) if !error.use_stderr() => {
            return match error.print() {
                Ok(()) => 0,
                Err(error) => print_error(&error.to_string()),
            };
        }
        Err(error) => {
            let message = error.to_string();
            let message = message.strip_prefix("error: ").unwrap_or(&message);
            let mut lines = message.lines();
            let status = print_error(lines.next().unwrap_or("Invalid arguments"));
            for line in lines {
                eprintln!("{}", redact(line));
            }
            return status;
        }
    };
    let mut log = match OperationalLog::start("revayat-subtitle", args.log_level) {
        Ok(log) => log,
        Err(error) => return print_error(&error),
    };
    log.record(LogLevel::Info, &format!("Command={}", args.command.name()));
    let status = match commands::execute(args.command) {
        Ok(status) => status,
        Err(error) => print_error(&error.to_string()),
    };
    log.finish(status);
    status
}

fn print_error(error: &str) -> i32 {
    eprintln!("ERROR: {}", redact(error));
    2
}
