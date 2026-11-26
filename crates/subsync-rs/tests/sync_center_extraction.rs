use std::{path::Path, process::Command, time::Duration};
use subsync_rs::audio::extract_pcm_via_ffmpeg_with_timeout;

fn multichannel_wav(path: &Path, center: &str) {
    let source = format!("aevalsrc=0.25|0|{center}|0|0|0|0|0:c=7.1(wide-side):s=16000:d=0.1");
    let output = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &source,
            "-c:a",
            "pcm_s16le",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wide_side_extracts_the_authored_front_center_samples() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wide-side.wav");
    multichannel_wav(&path, "0.125");
    let samples =
        extract_pcm_via_ffmpeg_with_timeout(&path, "ffmpeg", Duration::from_secs(5)).unwrap();
    assert_eq!(samples.len(), 1600);
    assert!(
        samples.iter().all(|&sample| sample == 4096),
        "first sample: {}",
        samples[0]
    );
}

#[test]
fn silent_front_center_uses_the_ordinary_mono_downmix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("silent-center.wav");
    multichannel_wav(&path, "0");
    let mono = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&path)
        .args(["-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
        .output()
        .unwrap();
    assert!(
        mono.status.success(),
        "{}",
        String::from_utf8_lossy(&mono.stderr)
    );
    let samples =
        extract_pcm_via_ffmpeg_with_timeout(&path, "ffmpeg", Duration::from_secs(5)).unwrap();
    assert!(samples.iter().any(|&sample| sample != 0));
    assert!(
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .eq(mono.stdout)
    );
}

#[cfg(unix)]
#[test]
fn expired_probe_budget_prevents_starting_another_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let fifo = directory.path().join("blocked-media");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    // The real ffprobe blocks on the FIFO. A fresh decode budget would try the missing executable.
    let subtitle = directory.path().join("source.srt");
    let aligned = directory.path().join("aligned.srt");
    let report = directory.path().join("report.json");
    std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\nTest\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_subsync"))
        .arg("--video")
        .arg(&fifo)
        .arg("--in")
        .arg(subtitle)
        .arg("--out")
        .arg(&aligned)
        .arg("--json-report")
        .arg(&report)
        .args([
            "--ffmpeg",
            "subsync-intentionally-missing-ffmpeg",
            "--ffmpeg-timeout",
            "0.25",
        ])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(error.contains("timed out after 0.25 seconds"), "{error}");
    assert!(!aligned.exists());
    assert!(!report.exists());
}
