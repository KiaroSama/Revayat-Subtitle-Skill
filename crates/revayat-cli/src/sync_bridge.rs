//! In-process subtitle synchronization and supervised executable fallback.

use clap::Args;
use revayat_core::process_control::{RunOptions, duration, run_owned};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use subsync_rs::{SyncConfig, SyncError, audio};

pub const SYNC_TIMEOUT: f64 = 300.0;
const AUDIO_LIMIT: u64 = (1 << 22) * 160 * 2 + 4096;
const SUBTITLE_LIMIT: u64 = 32 * 1024 * 1024;
const REPORT_LIMIT: usize = 8 * 1024 * 1024;
const WORKER_FLAG: &str = "--revayat-sync-worker";

#[derive(Args, Debug)]
#[command(group(clap::ArgGroup::new("media").args(["video", "wav"]).required(true)))]
pub struct SyncArgs {
    #[arg(long)]
    pub video: Option<PathBuf>,
    #[arg(long)]
    pub wav: Option<PathBuf>,
    #[arg(long = "in")]
    pub input: PathBuf,
    #[arg(long = "out")]
    pub output: PathBuf,
    #[arg(long, default_value = "ffmpeg")]
    pub ffmpeg: PathBuf,
    #[arg(long, default_value_t = 60.0)]
    pub max_offset_sec: f64,
    #[arg(long, default_value_t = 15000)]
    pub max_split_offset_ms: i64,
    #[arg(long, default_value_t = 7.0)]
    pub split_penalty: f64,
    #[arg(long, default_value_t = 350)]
    pub snap_ms: u64,
    #[arg(long)]
    pub no_fps_search: bool,
    #[arg(long)]
    pub json_report: Option<PathBuf>,
    #[arg(
        long,
        help = "Whole-operation deadline in seconds (default FFmpeg timeout: 300)"
    )]
    pub timeout: Option<f64>,
    #[arg(long, value_name = "PATH", conflicts_with = "sibling_subsync")]
    pub subsync: Option<PathBuf>,
    #[arg(long, help = "Run the subsync executable next to this binary")]
    pub sibling_subsync: bool,
    #[arg(long = "revayat-sync-worker", hide = true, requires = "wav", conflicts_with_all = ["subsync", "sibling_subsync", "timeout", "json_report"])]
    worker: bool,
}

#[derive(Clone, Copy)]
pub enum Source<'a> {
    Video(&'a Path),
    Wav(&'a Path),
}

#[derive(Clone, Copy, Default)]
pub enum Backend<'a> {
    #[default]
    InProcess,
    Supervised,
    Executable(&'a Path),
    Sibling,
}

pub struct SyncOptions<'a> {
    pub ffmpeg: &'a Path,
    pub max_offset_sec: f64,
    pub max_split_offset_ms: i64,
    pub split_penalty: f64,
    pub snap_ms: u64,
    pub fps_search: bool,
    pub json_report: Option<&'a Path>,
    pub timeout: f64,
    pub backend: Backend<'a>,
}

impl Default for SyncOptions<'_> {
    fn default() -> Self {
        Self {
            ffmpeg: Path::new("ffmpeg"),
            max_offset_sec: 60.0,
            max_split_offset_ms: 15000,
            split_penalty: 7.0,
            snap_ms: 350,
            fps_search: true,
            json_report: None,
            timeout: SYNC_TIMEOUT,
            backend: Backend::InProcess,
        }
    }
}

pub fn run_sync(args: SyncArgs) -> anyhow::Result<()> {
    let source = match (args.video.as_deref(), args.wav.as_deref()) {
        (Some(video), None) => Source::Video(video),
        (None, Some(wav)) => Source::Wav(wav),
        _ => anyhow::bail!("Choose exactly one of --video and --wav"),
    };
    let backend = match (args.subsync.as_deref(), args.sibling_subsync, args.timeout) {
        (Some(binary), false, _) => Backend::Executable(binary),
        (None, true, _) => Backend::Sibling,
        (None, false, Some(_)) => Backend::Supervised,
        (None, false, None) => Backend::InProcess,
        _ => anyhow::bail!("Choose at most one executable override"),
    };
    let data = sync_subtitle_to_video(
        source,
        &args.input,
        &args.output,
        &SyncOptions {
            ffmpeg: &args.ffmpeg,
            max_offset_sec: args.max_offset_sec,
            max_split_offset_ms: args.max_split_offset_ms,
            split_penalty: args.split_penalty,
            snap_ms: args.snap_ms,
            fps_search: !args.no_fps_search,
            json_report: args.json_report.as_deref(),
            timeout: args.timeout.unwrap_or(SYNC_TIMEOUT),
            backend: if args.worker {
                Backend::InProcess
            } else {
                backend
            },
        },
    )?;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &data)?;
    writeln!(stdout)?;
    Ok(())
}

pub fn locate_subsync(directory: &Path) -> io::Result<PathBuf> {
    executable(&directory.join(format!("subsync{}", std::env::consts::EXE_SUFFIX)))
}

fn executable(path: &Path) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    if !fs::metadata(&path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "subsync executable must be a regular file",
        ));
    }
    #[cfg(windows)]
    if !path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "subsync must be an executable file",
        ));
    }
    Ok(path)
}

fn path_text(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Synchronization requires UTF-8 paths"))
}

fn new_destination(path: &Path) -> anyhow::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
        Ok(_) => anyhow::bail!("Sync output must not already exist: {}", path.display()),
    }
    let leaf = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Missing sync output filename"))?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing sync output directory"))?;
    Ok(fs::canonicalize(parent)?.join(leaf))
}

fn bounded_file(path: &Path, limit: u64) -> anyhow::Result<fs::File> {
    let meta = fs::metadata(path)?;
    anyhow::ensure!(
        meta.is_file() && meta.len() <= limit,
        "Sync input is not a regular file or exceeds its size limit: {}",
        path.display()
    );
    Ok(fs::File::open(path)?)
}

pub fn sync_subtitle_to_video(
    source: Source<'_>,
    input: &Path,
    output: &Path,
    options: &SyncOptions<'_>,
) -> anyhow::Result<Value> {
    let began = Instant::now();
    let timeout = duration(
        options.timeout,
        "Sync timeout must be positive and at most 86400 seconds",
    )?;
    anyhow::ensure!(
        (0.0..=600.0).contains(&options.max_offset_sec),
        "--max-offset-sec must be within 0..=600"
    );
    let config = SyncConfig {
        max_offset_ms: (options.max_offset_sec * 1000.0).round() as i64,
        max_split_offset_ms: options.max_split_offset_ms,
        split_penalty: options.split_penalty,
        snap_ms: options.snap_ms,
        fps_search: options.fps_search,
    };
    config.validate()?;
    let input = std::path::absolute(input)?;
    let output = std::path::absolute(output)?;
    let report = options.json_report.map(std::path::absolute).transpose()?;
    let checked_output = new_destination(&output)?;
    let output_format = match output
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("srt") => Some(subsync_rs::SubtitleFormat::Srt),
        Some("ass" | "ssa") => Some(subsync_rs::SubtitleFormat::Ass),
        _ => None,
    };
    if let Some(path) = &report {
        let checked_report = new_destination(path)?;
        let mut same = checked_output == checked_report;
        if cfg!(windows) {
            same |= checked_output.to_string_lossy().to_lowercase()
                == checked_report.to_string_lossy().to_lowercase();
        }
        anyhow::ensure!(
            !same,
            "Sync subtitle and JSON report must use different output files"
        );
    }
    bounded_file(&input, SUBTITLE_LIMIT)?;
    let (is_video, media) = match source {
        Source::Video(path) => (true, std::path::absolute(path)?),
        Source::Wav(path) => {
            let path = std::path::absolute(path)?;
            bounded_file(&path, AUDIO_LIMIT)?;
            (false, path)
        }
    };
    let ffmpeg = path_text(options.ffmpeg)?;
    if matches!(options.backend, Backend::InProcess) {
        let data = if is_video {
            subsync_rs::sync_file_to_video_with_timeout(
                &media,
                &input,
                &output,
                ffmpeg,
                &config,
                report.as_deref(),
                timeout,
            )?
        } else {
            subsync_rs::sync_file_to_wav_with_report(
                &media,
                &input,
                &output,
                &config,
                report.as_deref(),
            )?
        };
        return Ok(serde_json::to_value(data)?);
    }
    let mut text = String::new();
    bounded_file(&input, SUBTITLE_LIMIT)?
        .take(SUBTITLE_LIMIT + 1)
        .read_to_string(&mut text)?;
    anyhow::ensure!(
        text.len() as u64 <= SUBTITLE_LIMIT,
        "Sync subtitle input exceeds its size limit"
    );
    let format = subsync_rs::SubtitleDocument::parse(&text, Some(&input))?.format;
    anyhow::ensure!(
        output_format.is_none_or(|output| output == format),
        "Output extension does not match the input subtitle format"
    );
    drop(text);
    let current = std::env::current_exe()?;
    let binary = match options.backend {
        Backend::Supervised => current.clone(),
        Backend::Executable(path) => executable(path)?,
        Backend::Sibling => locate_subsync(
            current
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing executable parent"))?,
        )?,
        Backend::InProcess => unreachable!(),
    };
    let remaining = || {
        timeout
            .checked_sub(began.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not sync subtitles: timed out after {} seconds",
                    options.timeout
                )
            })
    };
    let scratch = tempfile::tempdir()?;
    let wav = if is_video {
        // FFmpeg owns a separate process group; finish and reap it before starting the timed worker.
        let pcm = audio::extract_pcm_via_ffmpeg_with_timeout(&media, ffmpeg, remaining()?)?;
        let wav = scratch.path().join("audio.wav");
        audio::write_wav_pcm16(&wav, audio::VIDEO_SAMPLE_RATE, &pcm)?;
        wav
    } else {
        media.clone()
    };
    let staged_output = scratch.path().join(match format {
        subsync_rs::SubtitleFormat::Ass => "subtitle.ass",
        subsync_rs::SubtitleFormat::Srt => "subtitle.srt",
    });
    let mut command = vec![path_text(&binary)?.to_owned()];
    if matches!(options.backend, Backend::Supervised) {
        command.extend(["sync".into(), WORKER_FLAG.into()]);
    }
    command.extend([
        "--wav".into(),
        path_text(&wav)?.into(),
        "--in".into(),
        path_text(&input)?.into(),
        "--out".into(),
        path_text(&staged_output)?.into(),
        "--max-offset-sec".into(),
        options.max_offset_sec.to_string(),
        "--max-split-offset-ms".into(),
        options.max_split_offset_ms.to_string(),
        "--split-penalty".into(),
        options.split_penalty.to_string(),
        "--snap-ms".into(),
        options.snap_ms.to_string(),
    ]);
    if !options.fps_search {
        command.push("--no-fps-search".into());
    }
    let result = run_owned(
        &command,
        &RunOptions {
            timeout: remaining()?,
            max_output: REPORT_LIMIT,
            ..RunOptions::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("Could not sync subtitles: {e}"))?;
    let stdout = std::str::from_utf8(&result.stdout)?;
    let mut data = parse_report(stdout)?;
    data["video_path"] = if is_video { json!(media) } else { Value::Null };
    data["wav_path"] = if is_video { Value::Null } else { json!(media) };
    data["subtitle_in"] = json!(input);
    data["subtitle_out"] = json!(output);
    let subtitle = stage_file(&checked_output, |file| {
        let worker_output = fs::symlink_metadata(&staged_output)?;
        anyhow::ensure!(
            worker_output.is_file() && !worker_output.file_type().is_symlink(),
            "Sync worker did not produce a regular subtitle file"
        );
        let mut input = bounded_file(&staged_output, SUBTITLE_LIMIT)?.take(SUBTITLE_LIMIT + 1);
        let count = io::copy(&mut input, file)?;
        anyhow::ensure!(
            (1..=SUBTITLE_LIMIT).contains(&count),
            "Sync worker returned invalid subtitle size"
        );
        Ok(())
    })?;
    let receipt = report
        .as_deref()
        .map(|path| {
            let path = new_destination(path)?;
            let file = stage_file(&path, |file| {
                serde_json::to_writer_pretty(&mut *file, &data)?;
                writeln!(file)?;
                Ok(())
            })?;
            Ok::<_, anyhow::Error>((path, file))
        })
        .transpose()?;
    remaining()?;
    subtitle.persist_noclobber(&checked_output)?;
    if let Some((path, file)) = receipt {
        file.persist_noclobber(path)
            .map_err(|e| SyncError::ReportPublication {
                subtitle_path: output,
                source: Box::new(SyncError::io("Publish JSON report", e.error)),
            })?;
    }
    Ok(data)
}

fn stage_file(
    destination: &Path,
    write: impl FnOnce(&mut fs::File) -> anyhow::Result<()>,
) -> anyhow::Result<tempfile::NamedTempFile> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing sync output directory"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    write(file.as_file_mut())?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn parse_report(stdout: &str) -> anyhow::Result<Value> {
    let report: Value = serde_json::from_str(stdout.trim_start_matches('\u{feff}'))?;
    let numbers = [
        "audio_duration_ms",
        "cues_snapped",
        "cues_total",
        "fps_ratio",
        "global_offset_ms",
        "max_cue_delta_ms",
        "median_cue_delta_ms",
        "overlap_score_after",
        "overlap_score_before",
        "speech_segments_count",
    ];
    anyhow::ensure!(
        numbers.iter().all(|field| report[*field].is_number())
            && report["split_segments"].is_array(),
        "subsync returned an incomplete sync report"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_worker_report_is_refused() {
        assert!(parse_report("{\"cues_total\": 2, \"split_segments\": []}").is_err());
        assert!(parse_report("logging\n{}").is_err());
    }

    #[test]
    fn stages_both_files_before_declaring_a_worker_result() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let output = root.path().join("output.srt");
        let staged = stage_file(&output, |file| {
            file.write_all(b"owned bytes")?;
            Ok(())
        })?;
        fs::write(&output, b"independent winner")?;
        assert!(staged.persist_noclobber(&output).is_err());
        assert_eq!(fs::read(&output)?, b"independent winner");
        Ok(())
    }
}
