//! Command-line arguments for the standalone subtitle tools.

use crate::{install::InstallArgs, sync_bridge::SyncArgs};
use clap::{Args, Parser, Subcommand};
use revayat_core::runtime::LogLevel;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "revayat-subtitle",
    version,
    about = "Prepare, review, synchronize, and deliver subtitles",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "LEVEL",
        help = "Set the log level: DEBUG, INFO, WARNING, ERROR, CRITICAL"
    )]
    pub log_level: Option<LogLevel>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check FFmpeg and libass.
    Doctor {
        #[arg(long)]
        ffmpeg: Option<String>,
    },
    /// Import ASS, SRT, or ZIP files into a review workspace.
    Prepare(PrepareArgs),
    /// Validate review decisions and write an episode edition.
    Build {
        #[arg(long)]
        work: PathBuf,
        #[arg(long, value_parser = ["ass", "srt"])]
        output_format: Option<String>,
    },
    /// Convert SRT or ASS to Persian-ready ASS.
    Convert {
        #[command(flatten)]
        options: ConvertArgs,
        #[arg(long)]
        translator_mode: bool,
    },
    /// Create an ASS worksheet with missing donor slots.
    TranslatorAss(ConvertArgs),
    /// Synchronize subtitles against video or WAV audio.
    Sync(SyncArgs),
    /// Render visual samples using FFmpeg and libass.
    Render {
        #[arg(long)]
        build: PathBuf,
        #[arg(long)]
        episode: String,
        #[arg(long)]
        ffmpeg: Option<String>,
        #[arg(long)]
        video: Option<PathBuf>,
        #[arg(long)]
        fonts_dir: Option<PathBuf>,
        #[arg(long)]
        all_cues: bool,
    },
    /// Check a reviewed build without modifying it.
    Qa {
        #[arg(long)]
        build: PathBuf,
    },
    /// Package reviewed subtitles inside Sub/ in a new ZIP.
    Package {
        #[arg(long)]
        build: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Report whether a workspace is complete or pending.
    Status {
        #[arg(long)]
        work: PathBuf,
    },
    /// Validate revision-bound editorial proposals.
    Handoff {
        #[arg(long, required = true)]
        assignment: Vec<PathBuf>,
        #[arg(long, required = true)]
        result: Vec<PathBuf>,
        #[arg(long)]
        work: PathBuf,
    },
    /// Run a command with a deadline and clean up its process tree.
    Supervise {
        #[arg(long, value_name = "SECONDS")]
        timeout: f64,
        #[arg(last = true, required = true, num_args = 1.., value_name = "COMMAND")]
        command: Vec<String>,
    },
    /// Install, remove, or recover a skill or plugin.
    Install(InstallArgs),
    /// Check or fix Persian text mechanics.
    Lint(LintArgs),
    /// Find idioms and glossary terms in source text.
    Lookup(LookupArgs),
    /// Score answers against the evaluation rubric.
    Score {
        #[arg(long)]
        answers: PathBuf,
    },
}

impl Command {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Doctor { .. } => "doctor",
            Self::Prepare(_) => "prepare",
            Self::Build { .. } => "build",
            Self::Convert { .. } => "convert",
            Self::TranslatorAss(_) => "translator-ass",
            Self::Sync(_) => "sync",
            Self::Render { .. } => "render",
            Self::Qa { .. } => "qa",
            Self::Package { .. } => "package",
            Self::Status { .. } => "status",
            Self::Handoff { .. } => "handoff",
            Self::Supervise { .. } => "supervise",
            Self::Install(_) => "install",
            Self::Lint(_) => "lint",
            Self::Lookup(_) => "lookup",
            Self::Score { .. } => "score",
        }
    }
}

#[derive(Debug, Args)]
pub struct PrepareArgs {
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    #[arg(long)]
    pub work: PathBuf,
    #[arg(long)]
    pub series: String,
    #[arg(long, value_parser = season, allow_negative_numbers = true)]
    pub season: i64,
    #[arg(long, default_value = "utf-8-sig")]
    pub encoding: String,
    #[arg(long)]
    pub glossary: Option<PathBuf>,
    #[arg(long, default_value = "fa")]
    pub target_language: String,
    #[arg(long, default_value = "ass", value_parser = ["ass", "srt"])]
    pub output_format: String,
}

fn season(value: &str) -> Result<i64, &'static str> {
    value
        .parse::<i64>()
        .ok()
        .filter(|season| (1..=99).contains(season))
        .ok_or("Season must be an integer in 1..=99")
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    pub input: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long, default_value = "B Koodak")]
    pub font: String,
    #[arg(long, default_value_t = 110, allow_negative_numbers = true)]
    pub font_size: i64,
    #[arg(long, default_value_t = 2.0, allow_negative_numbers = true)]
    pub outline: f64,
    #[arg(long, default_value_t = 2.0, allow_negative_numbers = true)]
    pub shadow: f64,
    #[arg(long, default_value_t = 10, allow_negative_numbers = true)]
    pub margin_l: i64,
    #[arg(long, default_value_t = 10, allow_negative_numbers = true)]
    pub margin_r: i64,
    #[arg(long, default_value_t = 10, allow_negative_numbers = true)]
    pub margin_v: i64,
    #[arg(long, conflicts_with = "no_bold")]
    pub bold: bool,
    #[arg(long)]
    pub no_bold: bool,
    #[arg(long, default_value_t = 42, allow_negative_numbers = true)]
    pub max_chars: i64,
    #[arg(long)]
    pub keep_watermarks: bool,
    #[arg(long)]
    pub no_zwnj: bool,
    #[arg(long, default_value = "rtl", value_parser = ["rtl", "ltr"])]
    pub direction: String,
    #[arg(long)]
    pub donor: Vec<PathBuf>,
    #[arg(long)]
    pub source_comments: bool,
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long, conflicts_with = "fix")]
    pub check: bool,
    #[arg(long)]
    pub fix: bool,
    #[arg(long, requires = "fix")]
    pub in_place: bool,
    #[arg(long, conflicts_with = "fix")]
    pub json: bool,
    #[arg(long)]
    pub style: bool,
    #[arg(long, default_value = "persian", value_parser = ["persian", "latin", "keep"])]
    pub digits: String,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct LookupArgs {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub reverse: bool,
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    pub file_or_query: Option<String>,
}
