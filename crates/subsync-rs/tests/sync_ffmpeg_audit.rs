mod support;

use std::{fs, process::Command};
use support::{Fixture, INTERVALS, run};

#[test]
fn cli_exposes_ffmpeg_timeout() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_subsync"));
    command.arg("--help");
    let result = run(command);
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("--ffmpeg-timeout"));
}

#[test]
fn extracts_real_ffmpeg_matroska_audio() {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let media = fixture.directory.path().join("sample media.mkv");
    let mut command = Command::new("ffmpeg");
    command
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&fixture.wav)
        .args(["-c:a", "pcm_s16le"])
        .arg(&media);
    let result = run(command);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut command = fixture.command_for("--video", &media);
    command.args(["--end-adjust-ms", "0"]);
    let report = support::report(run(command));
    assert_eq!(report["cues_total"], INTERVALS.len());
    assert!(report["global_offset_ms"].as_i64().unwrap().abs_diff(-1300) <= 100);
    assert!(report["overlap_score_after"].as_f64().unwrap() >= 0.9);
    assert_eq!(
        report,
        serde_json::from_slice::<serde_json::Value>(&fs::read(&fixture.receipt).unwrap()).unwrap()
    );
    let wav_fixture = Fixture::new();
    wav_fixture.write_subtitle("\n");
    let mut wav_command = wav_fixture.command();
    wav_command.args(["--end-adjust-ms", "0"]);
    let wav_report = support::report(run(wav_command));
    assert_eq!(report["global_offset_ms"], wav_report["global_offset_ms"]);
    assert_eq!(
        fs::read(fixture.output).unwrap(),
        fs::read(wav_fixture.output).unwrap()
    );
}

#[test]
#[cfg(unix)]
fn times_out_on_real_ffmpeg_read_and_never_publishes() {
    use std::time::{Duration, Instant};

    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let fifo = fixture.directory.path().join("blocked media.mkv");
    let mut mkfifo = Command::new("mkfifo");
    mkfifo.arg(&fifo);
    assert!(run(mkfifo).status.success());
    let before = Instant::now();
    let mut command = fixture.command_for("--video", &fifo);
    command.args(["--ffmpeg-timeout", "0.5"]);
    let result = run(command);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("timed out"),
        "{result:?}"
    );
    assert!(before.elapsed() < Duration::from_secs(7));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("cleanup failed"));
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}
