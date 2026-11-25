//! Account-locked installation and authenticated recovery.

mod cli;
pub mod distribution;
pub mod files;
pub mod journal;
pub mod recovery;
pub mod registry;
pub mod transaction;

pub use cli::{InstallArgs, install, run_install};

#[cfg(test)]
mod tests;
