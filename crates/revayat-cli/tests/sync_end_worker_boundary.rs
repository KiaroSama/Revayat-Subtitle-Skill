#![cfg(unix)]

use anyhow::Result;
use revayat_core::process_control::{RunOptions, run_owned};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap, f64::consts::TAU, fs, os::unix::fs::PermissionsExt, path::Path,
    process::Output, time::Duration,
};
use subsync_rs::{SubtitleDocument, SubtitleFormat, SyncConfig};

fn inputs(root: &Path, format: SubtitleFormat) -> Result<String> {
    let pcm = (0..8000_u64 * 16)
        .map(|sample| {
            if (1560 * 16..2200 * 16).contains(&sample) {
                [400.0, 1200.0, 2400.0]
                    .iter()
                    .map(|frequency| 3500.0 * (TAU * frequency * sample as f64 / 16000.0).sin())
                    .sum::<f64>() as i16
            } else {
                (sample * 4093 % 131) as i16 - 65
            }
        })
        .collect::<Vec<_>>();
    subsync_rs::audio::write_wav_pcm16(&root.join("audio.wav"), 16000, &pcm)?;
    let extension = if format == SubtitleFormat::Srt {
        "srt"
    } else {
        "ass"
    };
    let text = if format == SubtitleFormat::Srt {
        "1\r\n00:00:01,000 --> 00:00:02,000\r\nAuthored\r\n"
    } else {
        "[Events]\r\nFormat: Start, End, Text\r\nDialogue: 0:00:01.00,0:00:02.00,Authored\r\n"
    };
    fs::write(root.join(format!("source.{extension}")), text)?;
    Ok(extension.to_owned())
}

fn command(root: &Path, extension: &str, helper: &Path) -> Result<Output> {
    let argv = vec![
        env!("CARGO_BIN_EXE_revayat-subtitle").to_owned(),
        "sync".into(),
        "--wav".into(),
        "audio.wav".into(),
        "--in".into(),
        format!("source.{extension}"),
        "--out".into(),
        format!("output.{extension}"),
        "--json-report".into(),
        "receipt.json".into(),
        "--max-offset-sec".into(),
        "0".into(),
        "--max-split-offset-ms".into(),
        "0".into(),
        "--snap-ms".into(),
        "1200".into(),
        "--end-adjust-ms".into(),
        "600".into(),
        "--no-fps-search".into(),
        "--subsync".into(),
        helper.to_string_lossy().into_owned(),
        "--timeout".into(),
        "5".into(),
    ];
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp)?;
    let env = BTreeMap::from([
        ("TMPDIR".into(), tmp.to_string_lossy().into_owned()),
        (
            "REVAYAT_LOG_DIR".into(),
            root.join("logs").to_string_lossy().into_owned(),
        ),
        (
            "REVAYAT_END_WORKER_PAYLOAD".into(),
            root.join("worker-payload").to_string_lossy().into_owned(),
        ),
        (
            "REVAYAT_END_WORKER_REPORT".into(),
            root.join("worker-report").to_string_lossy().into_owned(),
        ),
    ]);
    Ok(run_owned(
        &argv,
        &RunOptions {
            cwd: Some(root),
            timeout: Duration::from_secs(15),
            env: Some(&env),
            check: false,
            ..RunOptions::default()
        },
    )?)
}

#[test]
fn impossible_snap_before_end_extension_and_false_counts_leave_destinations_retryable() -> Result<()>
{
    for format in [SubtitleFormat::Srt, SubtitleFormat::Ass] {
        for defect in [
            "short-presnap",
            "at-old-end",
            "past-old-end",
            "wrong-end-count",
        ] {
            let directory = tempfile::tempdir()?;
            let root = directory.path();
            let extension = inputs(root, format)?;
            let input = root.join(format!("source.{extension}"));
            let original = fs::read(&input)?;
            let reference_path = root.join(format!("reference.{extension}"));
            let reference = subsync_rs::sync_file_to_wav(
                &root.join("audio.wav"),
                &input,
                &reference_path,
                &SyncConfig {
                    max_offset_ms: 0,
                    max_split_offset_ms: 0,
                    snap_ms: 1200,
                    end_adjust_ms: 600,
                    fps_search: false,
                    ..SyncConfig::default()
                },
            )?;
            assert_eq!(reference.cues_snapped, 1);
            assert_eq!(reference.cues_end_adjusted, 1);
            let valid_report = serde_json::to_value(reference)?;
            let valid_bytes = fs::read(&reference_path)?;
            let mut report = valid_report.clone();
            let bytes = if defect == "wrong-end-count" {
                report["cues_end_adjusted"] = json!(0);
                valid_bytes.clone()
            } else {
                let start = match defect {
                    "short-presnap" => 1700,
                    "at-old-end" => 2000,
                    "past-old-end" => 2100,
                    _ => unreachable!(),
                };
                let mut document = SubtitleDocument::parse(std::str::from_utf8(&original)?, None)?;
                document.cues[0].start_ms = start;
                document.cues[0].end_ms = 2600;
                report["cues_snapped"] = json!(1);
                report["cues_end_adjusted"] = json!(1);
                report["max_cue_delta_ms"] = json!(start - 1000);
                report["median_cue_delta_ms"] = json!((start - 1000) as f64);
                document.serialize()?.into_bytes()
            };
            fs::write(root.join("worker-payload"), bytes)?;
            fs::write(root.join("worker-report"), serde_json::to_vec(&report)?)?;
            let helper = root.join("authored-end-worker.sh");
            fs::write(
                &helper,
                r##"#!/bin/sh
set -eu
log() {
    level=$1
    shift
    case "$level" in DEBUG|INFO|WARNING|ERROR) ;; *) level=ERROR ;; esac
    printf '%s [%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$level" "$*" >&2
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) output=$2; shift 2 ;;
        *) shift ;;
    esac
done
log DEBUG 'Writing authored end-adjustment worker fixture'
if ! cat "$REVAYAT_END_WORKER_PAYLOAD" > "$output"; then
    log ERROR 'Could not write authored worker payload'
    exit 1
fi
log INFO 'Returning authored worker report for protocol validation'
cat "$REVAYAT_END_WORKER_REPORT"
"##,
            )?;
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))?;
            let failed = command(root, &extension, &helper)?;
            assert!(
                !failed.status.success(),
                "Accepted {defect} in {format:?}: {}",
                String::from_utf8_lossy(&failed.stdout)
            );
            assert!(!root.join(format!("output.{extension}")).exists());
            assert!(!root.join("receipt.json").exists());
            assert_eq!(fs::read(&input)?, original);
            fs::write(root.join("worker-payload"), &valid_bytes)?;
            fs::write(
                root.join("worker-report"),
                serde_json::to_vec(&valid_report)?,
            )?;
            let retry = command(root, &extension, &helper)?;
            assert!(
                retry.status.success(),
                "{defect} retry in {format:?}: {}",
                String::from_utf8_lossy(&retry.stderr)
            );
            assert_eq!(
                fs::read(root.join(format!("output.{extension}")))?,
                valid_bytes
            );
            let receipt: Value = serde_json::from_slice(&fs::read(root.join("receipt.json"))?)?;
            assert_eq!(receipt["cues_snapped"], 1);
            assert_eq!(receipt["cues_end_adjusted"], 1);
            assert_eq!(fs::read(&input)?, original);
        }
    }
    Ok(())
}
