mod support;

use std::{f64::consts::TAU, fs};
use subsync_rs::audio::write_wav_pcm16;
use support::{Fixture, run, tone};

fn invalid_option(key: &str, value: &str) {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let mut command = fixture.command();
    command.args([key, value]);
    let result = run(command);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .to_lowercase()
            .contains("configuration"),
        "{result:?}"
    );
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[test]
fn refuses_negative_global_offset() {
    invalid_option("--max-offset-sec", "-1");
}
#[test]
fn refuses_negative_local_offset() {
    invalid_option("--max-split-offset-ms", "-1");
}
#[test]
fn refuses_nan_split_penalty() {
    invalid_option("--split-penalty", "NaN");
}
#[test]
fn refuses_negative_split_penalty() {
    invalid_option("--split-penalty", "-1");
}
#[test]
fn refuses_snap_outside_bound() {
    invalid_option("--snap-ms", "5001");
}

#[test]
fn continuous_tone_does_not_publish_a_spurious_fps_change() {
    let mut fixture = Fixture::new();
    fs::write(
        &fixture.subtitle,
        b"1\n00:00:01,000 --> 00:00:02,000\nText\n",
    )
    .unwrap();
    fixture.wav = fixture.directory.path().join("continuous.wav");
    let tone = (0..160_000)
        .map(|i| (9000.0 * (TAU * 400.0 * f64::from(i) / 16_000.0).sin()) as i16)
        .collect::<Vec<_>>();
    write_wav_pcm16(&fixture.wav, 16_000, &tone).unwrap();
    let mut command = fixture.command();
    // Leave FPS search enabled to exercise the false boundary selection this used to trigger.
    let args = command
        .get_args()
        .filter(|arg| *arg != "--no-fps-search")
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    command = std::process::Command::new(env!("CARGO_BIN_EXE_subsync"));
    command.args(args).current_dir(fixture.directory.path());
    let result = run(command);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("continuous"),
        "{result:?}"
    );
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[test]
fn sparse_track_with_one_short_activity_burst_cannot_anchor_alignment() {
    let mut fixture = Fixture::new();
    fixture.write_subtitle("\n");
    fixture.wav = fixture.directory.path().join("sparse.wav");
    let samples = (0..30 * 16_000)
        .map(|i| {
            if (16_000..17_920).contains(&i) {
                tone(i)
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    write_wav_pcm16(&fixture.wav, 16_000, &samples).unwrap();
    let result = run(fixture.command());
    assert!(!result.status.success(), "{result:?}");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("too little speech"),
        "{result:?}"
    );
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());

    fixture.wav = fixture.directory.path().join("short usable.wav");
    subsync_rs::audio::write_wav_pcm16(&fixture.wav, 16_000, &samples[..32_000]).unwrap();
    assert_eq!(support::report(run(fixture.command()))["cues_total"], 4);
}
