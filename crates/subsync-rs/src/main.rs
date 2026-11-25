#![forbid(unsafe_code)]

use std::{
    env,
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};
use subsync_rs::{
    SyncConfig, SyncError, audio::DEFAULT_FFMPEG_TIMEOUT, sync_file_to_video_with_timeout,
    sync_file_to_wav_with_report,
};

const USAGE: &str = "Usage: subsync (--video PATH | --wav PATH) --in PATH --out PATH\n       [--ffmpeg PATH] [--ffmpeg-timeout 300] [--max-offset-sec 60]\n       [--max-split-offset-ms 15000] [--split-penalty 7.0] [--snap-ms 350] [--lead-in-ms 50]\n       [--no-fps-search] [--json-report PATH]";

fn invalid(message: impl Into<String>) -> SyncError {
    SyncError::Config(message.into())
}

fn parse<T: std::str::FromStr>(value: OsString, option: &str) -> Result<T, SyncError> {
    value
        .to_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| invalid(format!("{option} requires a number")))
}

fn write_version(flag: &std::ffi::OsStr, mut output: impl Write) -> Result<bool, SyncError> {
    if flag != "--version" && flag != "-V" {
        return Ok(false);
    }
    writeln!(output, "subsync {}", env!("CARGO_PKG_VERSION"))
        .map_err(|e| SyncError::io("Write version", e))?;
    Ok(true)
}

fn run() -> Result<(), SyncError> {
    let mut args = env::args_os().skip(1).peekable();
    if let Some(flag) = args.peek()
        && write_version(flag, io::stdout().lock())?
    {
        return Ok(());
    }
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        writeln!(io::stdout().lock(), "{USAGE}").map_err(|e| SyncError::io("Write help", e))?;
        return Ok(());
    }
    let mut config = SyncConfig::default();
    let (mut video, mut wav, mut input, mut output, mut json_report) =
        (None, None, None, None, None);
    let mut ffmpeg = "ffmpeg".to_owned();
    let mut ffmpeg_timeout = DEFAULT_FFMPEG_TIMEOUT;
    while let Some(flag) = args.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| invalid("option names must be UTF-8"))?;
        if flag == "--no-fps-search" {
            config.fps_search = false;
            continue;
        }
        let mut value = || {
            args.next()
                .filter(|arg| !arg.to_string_lossy().starts_with("--"))
                .ok_or_else(|| invalid(format!("{flag} requires a value")))
        };
        match flag {
            "--video" => video = Some(PathBuf::from(value()?)),
            "--wav" => wav = Some(PathBuf::from(value()?)),
            "--in" => input = Some(PathBuf::from(value()?)),
            "--out" => output = Some(PathBuf::from(value()?)),
            "--json-report" => json_report = Some(PathBuf::from(value()?)),
            "--ffmpeg" => {
                ffmpeg = value()?
                    .into_string()
                    .map_err(|_| invalid("--ffmpeg path must be UTF-8"))?
            }
            "--ffmpeg-timeout" => {
                let seconds: f64 = parse(value()?, flag)?;
                ffmpeg_timeout = Duration::try_from_secs_f64(seconds)
                    .ok()
                    .filter(|time| !time.is_zero() && time.as_secs_f64() <= 86_400.0)
                    .ok_or_else(|| {
                        invalid("--ffmpeg-timeout must be greater than 0 and at most 86400 seconds")
                    })?;
            }
            "--max-offset-sec" => {
                let seconds: f64 = parse(value()?, flag)?;
                if !(0.0..=600.0).contains(&seconds) {
                    return Err(invalid("--max-offset-sec must be within 0..=600"));
                }
                config.max_offset_ms = (seconds * 1000.0).round() as i64;
            }
            "--max-split-offset-ms" => config.max_split_offset_ms = parse(value()?, flag)?,
            "--split-penalty" => config.split_penalty = parse(value()?, flag)?,
            "--snap-ms" => config.snap_ms = parse(value()?, flag)?,
            "--lead-in-ms" => config.lead_in_ms = parse(value()?, flag)?,
            _ => return Err(invalid(format!("unknown option: {flag}\n{USAGE}"))),
        }
    }
    let input = input.ok_or_else(|| invalid("--in is required"))?;
    let output = output.ok_or_else(|| invalid("--out is required"))?;
    if let Some(path) = &json_report {
        if path
            .try_exists()
            .map_err(|e| SyncError::io("Check JSON report output", e))?
        {
            return Err(invalid("--json-report must point to a new file"));
        }
        let parent = |path: &std::path::Path| {
            std::fs::canonicalize(
                path.parent()
                    .filter(|dir| !dir.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new(".")),
            )
            .map_err(|e| SyncError::io("Open output directory", e))
        };
        let same_name = if cfg!(windows) {
            path.file_name()
                .zip(output.file_name())
                .is_some_and(|(a, b)| {
                    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
                })
        } else {
            path.file_name() == output.file_name()
        };
        if same_name && parent(path)? == parent(&output)? {
            return Err(invalid("--out and --json-report must name different files"));
        }
        parent(path)?;
    }
    let report = match (video, wav) {
        (Some(video), None) => sync_file_to_video_with_timeout(
            &video,
            &input,
            &output,
            &ffmpeg,
            &config,
            json_report.as_deref(),
            ffmpeg_timeout,
        )?,
        (None, Some(wav)) => {
            sync_file_to_wav_with_report(&wav, &input, &output, &config, json_report.as_deref())?
        }
        _ => return Err(invalid("choose exactly one of --video or --wav")),
    };
    let mut json = serde_json::to_vec_pretty(&report).map_err(SyncError::Json)?;
    json.push(b'\n');
    io::stdout()
        .lock()
        .write_all(&json)
        .map_err(|e| SyncError::io("Write JSON report", e))
}

fn main() -> ExitCode {
    let result = ctrlc::set_handler(subsync_rs::request_cancel)
        .map_err(|error| SyncError::io("Install cancellation handler", io::Error::other(error)))
        .and_then(|()| run());
    match result {
        Ok(()) if subsync_rs::check_cancelled().is_err() => ExitCode::from(130),
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "subsync: {error}");
            if subsync_rs::check_cancelled().is_err() {
                ExitCode::from(130)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_version_flags_write_the_release_version() -> Result<(), SyncError> {
        for flag in ["--version", "-V"] {
            let mut output = Vec::new();
            assert!(write_version(flag.as_ref(), &mut output)?);
            assert_eq!(output, b"subsync 3.0.2\n");
        }
        let mut output = Vec::new();
        assert!(!write_version("-v".as_ref(), &mut output)?);
        assert!(output.is_empty());
        Ok(())
    }
}
