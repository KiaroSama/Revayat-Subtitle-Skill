use anyhow::Result;
use revayat_core::process_control::{RunOptions, run_owned};
use serde_json::Value;
use std::{
    collections::BTreeMap, f64::consts::TAU, fs, path::Path, process::Output, time::Duration,
};

fn authored_inputs(root: &Path) -> Result<()> {
    let intervals = [(350_u64, 970_u64), (1590, 2630), (3710, 4480), (5110, 6640)];
    let pcm = (0..8 * 16000_u64)
        .map(|i| {
            if intervals
                .iter()
                .any(|&(a, b)| (a * 16..b * 16).contains(&i))
            {
                [400.0, 1200.0, 2400.0]
                    .iter()
                    .map(|f| 3500.0 * (TAU * f * i as f64 / 16000.0).sin())
                    .sum::<f64>() as i16
            } else {
                (i * 4093 % 131) as i16 - 65
            }
        })
        .collect::<Vec<_>>();
    subsync_rs::audio::write_wav_pcm16(&root.join("audio.wav"), 16000, &pcm)?;
    let time = |ms| format!("00:00:{:02},{:03}", ms / 1000, ms % 1000);
    let text = intervals
        .iter()
        .enumerate()
        .map(|(i, &(a, b))| {
            format!(
                "{}\r\n{} --> {}\r\n<i>Authored {i}</i>\r\n",
                i + 1,
                time(a + 1300),
                time(b + 1300)
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n");
    fs::write(root.join("source.srt"), text)?;
    Ok(())
}

fn command(root: &Path, extra: &[String], env: &BTreeMap<String, String>) -> Result<Output> {
    let mut argv = vec![
        env!("CARGO_BIN_EXE_revayat-subtitle").into(),
        "sync".into(),
        "--wav".into(),
        "audio.wav".into(),
        "--in".into(),
        "source.srt".into(),
        "--out".into(),
        "output.srt".into(),
        "--json-report".into(),
        "receipt.json".into(),
        "--no-fps-search".into(),
    ];
    argv.extend_from_slice(extra);
    let mut env = env.clone();
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp)?;
    for key in ["TEMP", "TMP", "TMPDIR"] {
        env.entry(key.into())
            .or_insert_with(|| tmp.to_string_lossy().into_owned());
    }
    env.insert(
        "REVAYAT_LOG_DIR".into(),
        root.join("logs").to_string_lossy().into_owned(),
    );
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
fn real_in_process_and_supervised_workers_return_the_same_complete_result() -> Result<()> {
    let mut outputs = Vec::new();
    for extra in [vec![], vec!["--timeout".into(), "5".into()]] {
        let root = tempfile::tempdir()?;
        authored_inputs(root.path())?;
        let source = fs::read(root.path().join("source.srt"))?;
        let output = command(root.path(), &extra, &BTreeMap::new())?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["cues_total"], 4);
        assert_eq!(
            report,
            serde_json::from_slice::<Value>(&fs::read(root.path().join("receipt.json"))?)?
        );
        assert_eq!(fs::read(root.path().join("source.srt"))?, source);
        outputs.push(fs::read(root.path().join("output.srt"))?);
    }
    assert_eq!(outputs[0], outputs[1]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn malformed_external_worker_results_leave_both_destinations_retryable() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for defect in [
        "text",
        "utf8",
        "changed-content",
        "negative-count",
        "wrong-count",
        "wrong-delta",
        "zero-bounds-shift",
        "zero-bounds-end",
    ] {
        let root = tempfile::tempdir()?;
        let root = root.path();
        authored_inputs(root)?;
        let zero_bounds = defect.starts_with("zero-bounds");
        if zero_bounds {
            fs::write(
                root.join("source.srt"),
                "1\n00:00:01,000 --> 00:00:02,000\nAuthored\n",
            )?;
        }
        let original = fs::read(root.join("source.srt"))?;
        let reference = subsync_rs::sync_file_to_wav(
            &root.join("audio.wav"),
            &root.join("source.srt"),
            &root.join("reference.srt"),
            &subsync_rs::SyncConfig {
                fps_search: false,
                max_offset_ms: if zero_bounds { 0 } else { 60000 },
                max_split_offset_ms: if zero_bounds { 0 } else { 15000 },
                snap_ms: if zero_bounds { 0 } else { 350 },
                ..subsync_rs::SyncConfig::default()
            },
        )?;
        let valid_report = serde_json::to_value(reference)?;
        let valid_bytes = fs::read(root.join("reference.srt"))?;
        let mut report = valid_report.clone();
        let bytes = match defect {
            "text" => b"worker output is not a subtitle".to_vec(),
            "utf8" => vec![0xff, 0xfe],
            "changed-content" => String::from_utf8(valid_bytes.clone())?
                .replace("Authored", "Changed")
                .into_bytes(),
            "negative-count" => {
                report["cues_total"] = serde_json::json!(-1);
                valid_bytes.clone()
            }
            "wrong-count" => {
                report["cues_total"] = serde_json::json!(3);
                valid_bytes.clone()
            }
            "wrong-delta" => {
                report["max_cue_delta_ms"] = serde_json::json!(999999);
                valid_bytes.clone()
            }
            "zero-bounds-shift" => {
                report["max_cue_delta_ms"] = serde_json::json!(3599000);
                report["median_cue_delta_ms"] = serde_json::json!(3599000.0);
                b"1\n01:00:00,000 --> 01:00:01,000\nAuthored\n".to_vec()
            }
            "zero-bounds-end" => b"1\n00:00:01,000 --> 01:00:01,000\nAuthored\n".to_vec(),
            _ => unreachable!(),
        };
        fs::write(root.join("worker-payload"), bytes)?;
        fs::write(root.join("worker-report"), serde_json::to_vec(&report)?)?;
        let helper = root.join("authored worker.sh");
        fs::write(
            &helper,
            r##"#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) output=$2; shift 2 ;;
        *) shift ;;
    esac
done
printf '%s [INFO] Writing authored worker fixture\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >&2
cat "$REVAYAT_TEST_WORKER_PAYLOAD" > "$output"
cat "$REVAYAT_TEST_WORKER_REPORT"
"##,
        )?;
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))?;
        let env = BTreeMap::from([
            (
                "REVAYAT_TEST_WORKER_PAYLOAD".into(),
                root.join("worker-payload").to_string_lossy().into_owned(),
            ),
            (
                "REVAYAT_TEST_WORKER_REPORT".into(),
                root.join("worker-report").to_string_lossy().into_owned(),
            ),
        ]);
        let mut args = vec![
            "--subsync".into(),
            helper.to_string_lossy().into_owned(),
            "--timeout".into(),
            "5".into(),
        ];
        if zero_bounds {
            args.extend(
                [
                    "--max-offset-sec",
                    "0",
                    "--max-split-offset-ms",
                    "0",
                    "--snap-ms",
                    "0",
                ]
                .map(str::to_owned),
            );
        }
        let result = command(root, &args, &env)?;
        assert!(!result.status.success(), "Accepted {defect}");
        assert!(
            !root.join("output.srt").exists(),
            "Published {defect} subtitle"
        );
        assert!(
            !root.join("receipt.json").exists(),
            "Published {defect} report"
        );
        assert_eq!(fs::read(root.join("source.srt"))?, original);
        fs::write(root.join("worker-payload"), &valid_bytes)?;
        fs::write(
            root.join("worker-report"),
            serde_json::to_vec(&valid_report)?,
        )?;
        let retry = command(root, &args, &env)?;
        assert!(
            retry.status.success(),
            "{defect} retry: {}",
            String::from_utf8_lossy(&retry.stderr)
        );
        assert_eq!(fs::read(root.join("output.srt"))?, valid_bytes);
        let receipt: Value = serde_json::from_slice(&fs::read(root.join("receipt.json"))?)?;
        assert_eq!(receipt["cues_total"], if zero_bounds { 1 } else { 4 });
    }
    Ok(())
}
