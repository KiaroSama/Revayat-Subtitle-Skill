//! Wire CLI inputs and reports to the subtitle tools.

mod voice;

use crate::{
    cli::{Command, ConvertArgs, PrepareArgs},
    install, sync_bridge,
};
use anyhow::{Result, anyhow, ensure};
use revayat_core::{
    evaluation, handoff,
    process_control::{self, RunError, RunOptions},
    publication::publish_bytes,
    render::{delivery, renderer},
    runtime::{diagnostic::redact, files},
    status,
    subtitle_formats::{self, SubtitleDocument, conversion::Options, document},
    workflow::{import, workspace},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) const TEXT_LIMIT: u64 = 16 * 1024 * 1024;

pub fn execute(command: Command) -> Result<i32> {
    let report = match command {
        Command::Doctor { ffmpeg } => renderer::doctor(ffmpeg.as_deref()),
        Command::Prepare(args) => prepare(args),
        Command::Build {
            work,
            output_format,
        } => workspace::build(&work, output_format.as_deref()),
        Command::Convert {
            options,
            translator_mode,
        } => return write_ready(convert(options, translator_mode)?),
        Command::TranslatorAss(options) => return write_ready(convert(options, true)?),
        Command::Sync(args) => return sync_bridge::run_sync(args).map(|()| 0),
        Command::Render {
            build,
            episode,
            ffmpeg,
            video,
            fonts_dir,
            all_cues,
        } => renderer::render(
            &build,
            &episode,
            ffmpeg.as_deref(),
            video.as_deref(),
            fonts_dir.as_deref(),
            all_cues,
        ),
        Command::Qa { build } => delivery::qa(&build),
        Command::Package { build, out } => delivery::package(&build, &std::path::absolute(out)?),
        Command::Status { work } => status::report(&work),
        Command::Handoff {
            work,
            assignment,
            result,
        } => handoff::review_handoff(&work, &assignment, &result),
        Command::Supervise { timeout, command } => return supervise(timeout, &command),
        Command::Install(args) => return install::run_install(args).map(|()| 0),
        Command::Lint(args) => return voice::lint(args),
        Command::Lookup(args) => return voice::lookup(args),
        Command::Score { answers } => {
            let answers = files::read_json(&answers, files::MAX_JSON_BYTES).map_err(core_error)?;
            let report = evaluation::score(answers).map_err(core_error)?;
            let counts = &report["counts"];
            let needs_review = ["needs_review", "missing"]
                .iter()
                .any(|key| counts[*key].as_u64().is_some_and(|count| count > 0));
            write_json(&report)?;
            return Ok(i32::from(needs_review));
        }
    };
    write_ready(report.map_err(core_error)?)
}

pub(crate) fn core_error(error: String) -> anyhow::Error {
    anyhow!(error)
}

pub(crate) fn write_json(report: &Value) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn write_ready(report: Value) -> Result<i32> {
    write_json(&report)?;
    Ok(i32::from(report["ready"] == false))
}

fn prepare(args: PrepareArgs) -> Result<Value, String> {
    let work = std::path::absolute(args.work).map_err(|error| error.to_string())?;
    import::prepare(
        &args.inputs,
        &work,
        &import::ImportOptions {
            series: &args.series,
            season: args.season,
            encoding: &args.encoding,
            glossary: args.glossary.as_deref(),
            language: &args.target_language,
            output_format: &args.output_format,
        },
    )
}

fn load_subtitle(path: &Path) -> Result<SubtitleDocument> {
    let kind = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    ensure!(
        matches!(kind.as_str(), "srt" | "ass"),
        "Conversion requires a regular .srt or .ass input file"
    );
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !files::linked(&metadata),
        "Conversion requires a regular .srt or .ass input file"
    );
    let raw = files::read_limited(path, TEXT_LIMIT).map_err(core_error)?;
    let raw = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw);
    document::parse(std::str::from_utf8(raw)?, &kind).map_err(core_error)
}

fn convert(args: ConvertArgs, translator_mode: bool) -> Result<Value> {
    ensure!(
        args.out
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ass")),
        "Conversion output must use the .ass extension"
    );
    ensure!(
        translator_mode || (args.donor.is_empty() && !args.source_comments),
        "Donors and source comments require translator mode"
    );
    ensure!(
        args.out.to_str().is_some(),
        "Output path is not valid UTF-8"
    );
    ensure!(
        args.outline.is_finite() && args.shadow.is_finite(),
        "Outline and shadow must be finite numbers"
    );
    let style = subtitle_formats::build_ass_style(
        "Default".into(),
        Some(json!(args.font)),
        52,
        Some(json!(args.outline)),
        Some(json!(args.shadow)),
        None,
        None,
        json!(-1),
        2,
        24,
    )
    .map_err(core_error)?;
    let options = Options {
        style: Some(&style),
        direction: &args.direction,
        fix_zwnj: !args.no_zwnj,
        strip_watermarks: !args.keep_watermarks,
        max_chars: args.max_chars,
        ..Options::default()
    };
    let primary = load_subtitle(&args.input)?;
    let (doc, stats) = if translator_mode {
        let donors = args
            .donor
            .iter()
            .map(|path| {
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or_else(|| anyhow!("Donor filename is not valid UTF-8"))?;
                Ok((stem.to_owned(), load_subtitle(path)?))
            })
            .collect::<Result<Vec<_>>>()?;
        subtitle_formats::translator_ass(&primary, &donors, args.source_comments, &options)
            .map_err(core_error)?
    } else {
        let (doc, removed) = subtitle_formats::convert(&primary, &options).map_err(core_error)?;
        (doc, json!({"watermarks_removed": removed.len()}))
    };
    let payload = document::serialize(&doc, &doc.cues, false).map_err(core_error)?;
    ensure!(
        document::parse(&payload, "ass")
            .map_err(core_error)?
            .cues
            .len()
            == doc.cues.len(),
        "Output cue count changed during serialization"
    );
    publish_bytes(&args.out, payload.as_bytes())?;
    let mut report = json!({"output": args.out, "cues": doc.cues.len()});
    if let (Some(report), Value::Object(stats)) = (report.as_object_mut(), stats) {
        report.extend(stats);
    }
    Ok(report)
}

fn supervise(timeout: f64, command: &[String]) -> Result<i32> {
    let options = RunOptions {
        timeout: process_control::duration(timeout, "timeout")?,
        check: false,
        ..RunOptions::default()
    };
    match process_control::run_owned(command, &options) {
        Ok(output) => {
            io::stdout().lock().write_all(&output.stdout)?;
            io::stderr().lock().write_all(&output.stderr)?;
            Ok(process_control::exit_code(&output))
        }
        Err(error @ RunError::Timeout { .. }) => {
            let _ = writeln!(io::stderr(), "ERROR: {}", redact(&error.to_string()));
            Ok(124)
        }
        Err(error) => Err(error.into()),
    }
}
