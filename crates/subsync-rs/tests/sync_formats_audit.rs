mod support;

use std::{fs, path::Path};
use subsync_rs::SubtitleDocument;
use support::{CAPTIONS, Fixture, INTERVALS, report, run};

fn assert_report(fixture: &Fixture, report: &serde_json::Value, shift: i64) {
    assert_eq!(report["cues_total"], INTERVALS.len());
    assert!(
        report["global_offset_ms"]
            .as_i64()
            .unwrap()
            .abs_diff(-shift)
            <= 100
    );
    assert_eq!(report["fps_ratio"], 1.0);
    let before = report["overlap_score_before"].as_f64().unwrap();
    let after = report["overlap_score_after"].as_f64().unwrap();
    assert!(after > before && after >= 0.9, "{report}");
    let segments = report["split_segments"].as_array().unwrap();
    assert_eq!(segments[0]["start_cue"], 1);
    assert_eq!(segments.last().unwrap()["end_cue"], INTERVALS.len());
    assert_eq!(
        *report,
        serde_json::from_slice::<serde_json::Value>(&fs::read(&fixture.receipt).unwrap()).unwrap()
    );
}

fn align_srt(newline: &str) {
    let fixture = Fixture::new();
    let original = format!("\u{feff}{}", fixture.srt(1300, newline));
    fs::write(&fixture.subtitle, &original).unwrap();
    let receipt = report(run(fixture.command()));
    assert_report(&fixture, &receipt, 1300);
    let actual = fs::read_to_string(&fixture.output).unwrap();
    let original = original.strip_prefix('\u{feff}').unwrap();
    assert_eq!(actual.matches(CAPTIONS).count(), INTERVALS.len());
    let lines_except_timings = |text: &str| {
        text.split_inclusive(['\r', '\n'])
            .filter(|line| !line.contains(" --> "))
            .collect::<String>()
    };
    assert_eq!(
        lines_except_timings(original),
        lines_except_timings(&actual)
    );
    let parsed = SubtitleDocument::parse(&actual, Some(&fixture.output)).unwrap();
    for (cue, &(begin, _)) in parsed.cues.iter().zip(&INTERVALS) {
        assert!(cue.start_ms.abs_diff(begin) < 250, "{cue:?}");
    }
    assert!(
        fs::read(&fixture.subtitle)
            .unwrap()
            .starts_with(b"\xef\xbb\xbf")
    );
}

#[test]
fn wav_preserves_unicode_srt_tags() {
    align_srt("\r\n");
}

#[test]
fn wav_preserves_unicode_ass_tags_and_comments() {
    let mut fixture = Fixture::new();
    fixture.subtitle.set_extension("ass");
    fixture.output.set_extension("ass");
    let header = "[Script Info]\r\nTitle: Sample\r\n[V4+ Styles]\r\n[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\nComment: 0,0:00:00.00,0:00:00.10,Default,,0,0,0,,Editorial note\r\n";
    let caption = r"{\i1}{\an8}سلام، دنیا";
    let timestamp = |ms: u64| format!("0:00:{:02}.{:02}", ms / 1000, ms % 1000 / 10);
    let document = format!(
        "\u{feff}{header}{}",
        INTERVALS
            .iter()
            .map(|&(begin, end)| {
                format!(
                    "Dialogue: 1,{},{},Default,A,0,0,0,,{caption}\r\n",
                    timestamp(begin - 250),
                    timestamp(end - 250)
                )
            })
            .collect::<String>()
    );
    fs::write(&fixture.subtitle, &document).unwrap();
    assert_report(&fixture, &report(run(fixture.command())), -250);
    let actual = fs::read_to_string(&fixture.output).unwrap();
    assert!(actual.starts_with(header));
    assert_eq!(actual.matches(caption).count(), INTERVALS.len());
    assert_eq!(fs::read_to_string(&fixture.subtitle).unwrap(), document);
}

#[test]
fn preserves_cr_only_line_stream() {
    align_srt("\r");
}
#[test]
fn preserves_crlf_line_stream() {
    align_srt("\r\n");
}
#[test]
fn preserves_lf_line_stream() {
    align_srt("\n");
}

#[test]
fn preserves_every_mixed_line_ending_including_last_byte() {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let original = fs::read_to_string(&fixture.subtitle).unwrap();
    let mixed = original
        .trim_end_matches('\n')
        .split('\n')
        .enumerate()
        .map(|(position, line)| {
            let ending = if position + 1 == original.lines().count() {
                ""
            } else {
                ["\r", "\r\n", "\n"][position % 3]
            };
            format!("{line}{ending}")
        })
        .collect::<String>();
    fs::write(&fixture.subtitle, &mixed).unwrap();
    assert_report(&fixture, &report(run(fixture.command())), 1300);
    let actual = fs::read_to_string(&fixture.output).unwrap();
    let nondigits = |text: &str| {
        text.bytes()
            .filter(|value| !value.is_ascii_digit())
            .collect::<Vec<_>>()
    };
    assert_eq!(nondigits(&actual), nondigits(&mixed));
    assert_ne!(actual, mixed);
}

fn missing_separator(numbered: bool) {
    let fixture = Fixture::new();
    let source = if numbered {
        "1\n00:00:01,000 --> 00:00:02,000\nFirst\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n"
    } else {
        "00:00:01,000 --> 00:00:02,000\nFirst\n00:00:03,000 --> 00:00:04,000\nSecond\n"
    };
    fs::write(&fixture.subtitle, source).unwrap();
    let result = run(fixture.command());
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("separator"),
        "{result:?}"
    );
    assert_eq!(fs::read_to_string(&fixture.subtitle).unwrap(), source);
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[test]
fn refuses_numbered_srt_without_separator() {
    missing_separator(true);
}
#[test]
fn refuses_unnumbered_srt_without_separator() {
    missing_separator(false);
}

fn check_skill_frontmatter(relative_path: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    let source = fs::read_to_string(root.join(relative_path).join("SKILL.md")).unwrap();
    let mut parts = source.splitn(3, "---\n");
    assert_eq!(parts.next(), Some(""));
    let frontmatter = parts.next().unwrap();
    assert!(
        frontmatter
            .lines()
            .any(|line| line == "name: revayat-subtitle-sync")
    );
    let description = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .unwrap();
    assert!(!description.is_empty() && description.chars().count() <= 1024);
    assert!(parts.next().unwrap().to_lowercase().contains("sync"));
}

#[test]
fn nested_sync_skill_frontmatter() {
    check_skill_frontmatter("revayat-subtitle/sync");
}
#[test]
fn standalone_sync_skill_frontmatter() {
    check_skill_frontmatter("revayat-subtitle-sync");
}
