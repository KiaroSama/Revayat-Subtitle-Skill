mod support;

use std::{fs, path::PathBuf, process::Command, time::Duration};
use subsync_rs::audio::{extract_pcm_via_ffmpeg_with_timeout, read_wav_pcm16};
use support::{Fixture, report, run};

fn media(fixture: &Fixture, offset: &str, gap: bool) -> PathBuf {
    let path = fixture.directory.path().join("authored timeline.mkv");
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:r=1:d=10",
            "-itsoffset",
            offset,
            "-i",
        ])
        .arg(&fixture.wav)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-avoid_negative_ts",
            "disabled",
        ]);
    if gap {
        command.args(["-af", "asetnsamples=n=160:p=0,asetpts=PTS+gte(T\\,3)*2/TB"]);
    }
    command.arg(&path);
    let output = run(command);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

fn extract(path: &std::path::Path) -> Vec<i16> {
    extract_pcm_via_ffmpeg_with_timeout(path, "ffmpeg", Duration::from_secs(5)).unwrap()
}

fn assert_pcm(actual: &[i16], expected: &[i16], reason: &str) {
    assert_eq!(actual.len(), expected.len(), "{reason}");
    assert_eq!(
        actual.iter().zip(expected).position(|(a, b)| a != b),
        None,
        "{reason}: first differing sample"
    );
}

#[test]
fn delayed_audio_keeps_its_position_on_the_video_clock() {
    let fixture = Fixture::new();
    let path = media(&fixture, "2", false);
    let original = read_wav_pcm16(&fixture.wav).unwrap().1;
    let mut expected = vec![0; 2 * 16000];
    expected.extend(original);
    assert_pcm(
        &extract(&path),
        &expected,
        "Initial audio delay must become PCM silence",
    );
    fs::write(&fixture.subtitle, fixture.srt(2000, "\n")).unwrap();
    let receipt = report(run(fixture.command_for("--video", &path)));
    assert_eq!(receipt["audio_duration_ms"], 10000);
    assert!(receipt["global_offset_ms"].as_i64().unwrap().abs() <= 100);
    let output =
        subsync_rs::SubtitleDocument::parse(&fs::read_to_string(&fixture.output).unwrap(), None)
            .unwrap();
    assert!(output.cues[0].start_ms.abs_diff(2350) <= 100);
}

#[test]
fn missing_audio_packets_become_silence_instead_of_shortening_the_clock() {
    let fixture = Fixture::new();
    let path = media(&fixture, "0", true);
    let original = read_wav_pcm16(&fixture.wav).unwrap().1;
    let mut expected = original[..3 * 16000].to_vec();
    expected.resize(expected.len() + 2 * 16000, 0);
    expected.extend_from_slice(&original[3 * 16000..]);
    assert_pcm(
        &extract(&path),
        &expected,
        "A timestamp gap must not shift all later dialogue",
    );
}

#[test]
fn zero_start_media_keeps_the_original_pcm_samples() {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    assert_eq!(report(run(fixture.command()))["cues_total"], 4);
    let path = media(&fixture, "0", false);
    assert_pcm(
        &extract(&path),
        &read_wav_pcm16(&fixture.wav).unwrap().1,
        "Zero-start PCM must be unchanged",
    );
}

#[test]
fn negative_container_start_is_normalized_to_the_playback_origin() {
    let fixture = Fixture::new();
    let path = media(&fixture, "-2", false);
    assert_pcm(
        &extract(&path),
        &read_wav_pcm16(&fixture.wav).unwrap().1,
        "Do not add silence for the container's negative absolute timestamp origin",
    );
}
