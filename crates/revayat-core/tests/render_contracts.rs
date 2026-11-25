use flate2::read::DeflateDecoder;
use revayat_core::{
    png_validation::decode_dimensions,
    process_control::{RunOptions, run_owned},
    render::{delivery, renderer, sampling},
    runtime::files::write_json,
    subtitle_formats::{
        conversion::{self, Options},
        document::{ASS_FIELDS, DEFAULT_STYLE, STYLE_FIELDS, parse, serialize},
    },
    workflow::{
        import::{ImportOptions, prepare},
        workspace::{build, digest, read_record},
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self as std_io, Read},
    path::{Path, PathBuf},
    time::Duration,
};

const EPISODE: &str = "S01E01";
const STYLE: &str = "Default,Arial,42,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,1,2,30,30,35,1";
const SRT: &str = "1\n00:00:01,001 --> 00:00:02,009\nx < 5 and y > 2\n";

fn ass(style: &str, headers: &str, events: &str) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n{headers}\n[V4+ Styles]\nFormat: {STYLE_FIELDS}\nStyle: {style}\n[Events]\nFormat: {ASS_FIELDS}\n{events}\n"
    )
}

fn event(start: u32, end: u32, text: &str) -> String {
    format!("Dialogue: 0,0:00:{start:02}.00,0:00:{end:02}.00,Default,,0,0,0,,{text}")
}

fn source(text: &str) -> String {
    ass(DEFAULT_STYLE, "", &event(1, 3, text))
}

fn workspace(
    inputs: &[(&str, &str)],
    language: &str,
) -> Result<(tempfile::TempDir, PathBuf), String> {
    let root = tempfile::tempdir().map_err(io)?;
    let work = root.path().join("فضای کار subtitle tests");
    let mut paths = Vec::new();
    for (name, text) in inputs {
        let path = root.path().join(name);
        fs::write(&path, text).map_err(io)?;
        paths.push(path);
    }
    prepare(
        &paths,
        &work,
        &ImportOptions {
            series: "Render fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language,
            output_format: if inputs[0].0.ends_with(".srt") {
                "srt"
            } else {
                "ass"
            },
        },
    )?;
    let mut project = read_record(&work, "project.json")?;
    project["episodes"] = json!([{"id":EPISODE,"base":"s0001","alternates":[],"comparison":"Authored render fixture"}]);
    write_json(&work.join("project.json"), &project)?;
    write_json(
        &work.join("glossary.json"),
        &json!({"series":"Render fixture","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored fixture, no media or terminology claims"}]}),
    )?;
    for index in 1..=paths.len() {
        let sheet = format!("worksheets/s{index:04}.json");
        let mut rows = read_record(&work, &sheet)?;
        for row in rows.as_array_mut().ok_or("expected worksheet rows")? {
            row["action"] = json!("preserve");
            row["reviewed"] = json!(true);
        }
        write_json(&work.join(sheet), &rows)?;
    }
    Ok((root, work))
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn path(value: &Value) -> Result<&Path, String> {
    value
        .as_str()
        .map(Path::new)
        .ok_or_else(|| format!("Expected a path, got {value}"))
}

fn edition(work: &Path) -> Result<PathBuf, String> {
    Ok(path(&build(work, None)?["build"])?.to_owned())
}

fn one_edition() -> Result<(tempfile::TempDir, PathBuf, PathBuf), String> {
    let (root, work) = workspace(&[("S01E01.ass", &source("VISIBLE AVAVAV"))], "en")?;
    let output = edition(&work)?;
    Ok((root, work, output))
}

fn episode(output: &Path) -> Result<Value, String> {
    Ok(read_record(output, "manifest.json")?["episodes"][0].clone())
}

fn paint(output: &Path, all: bool) -> Result<Value, String> {
    renderer::render(output, EPISODE, None, None, None, all)
}

fn inspect(output: &Path) -> Result<Value, String> {
    let mut review = read_record(output, "renders/S01E01.json")?;
    for frame in review["frames"].as_array_mut().ok_or("expected frames")? {
        frame["reviewed"] = json!(true);
        frame["note"] = json!("Mechanical pixel check; no human visual or language approval.");
    }
    write_json(&output.join("renders/S01E01.json"), &review)?;
    Ok(review)
}

fn receipt(output: &Path, evidence: &Value) -> Result<Value, String> {
    let review = read_record(output, "renders/S01E01.json")?;
    assert_eq!(evidence["frames"], review["frames"]);
    let relative = path(&review["receipt_file"])?;
    let raw = fs::read(output.join(relative)).map_err(io)?;
    assert_eq!(review["receipt_sha256"], digest(&raw));
    serde_json::from_slice(&raw).map_err(|error| error.to_string())
}

fn refuses<T>(result: Result<T, String>, reason: &str) -> String {
    let error = result.err().expect("expected refusal");
    assert!(error.contains(reason), "expected {reason:?}, got {error:?}");
    error
}

#[test]
fn reports_missing_ffmpeg_with_actionable_diagnostic() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io)?;
    let missing = root.path().join("missing-ffmpeg");
    let explicit = missing.to_str().ok_or("expected UTF-8 path")?;
    let report = renderer::doctor(Some(explicit))?;
    assert_eq!(report["ready"], false);
    let error = refuses(renderer::ffmpeg_path(Some(explicit)), "FFmpeg not found");
    assert!(error.contains("libass-enabled"));
    assert!(error.contains("REVAYAT_FFMPEG"));
    assert!(error.ends_with(report["error"].as_str().ok_or("expected diagnostic")?));
    Ok(())
}

#[test]
fn ffmpeg_provides_real_ass_subtitles_and_color_filters() -> Result<(), String> {
    let report = renderer::doctor(None)?;
    assert_eq!(report["ready"], true, "{report}");
    assert_eq!(report["libass"], true, "{report}");
    Ok(())
}

#[test]
fn rejects_invalid_episode_without_publishing_render_evidence() -> Result<(), String> {
    let (_root, _work, output) = one_edition()?;
    for invalid in ["S01E99", "../S01E01", "s01e01"] {
        refuses(
            renderer::render(&output, invalid, None, None, None, false),
            "Episode ID is not in this build",
        );
    }
    assert!(!output.join("renders").exists());
    Ok(())
}

#[test]
fn rejects_legacy_and_modified_builds_before_rendering() -> Result<(), String> {
    let (_root, _work, output) = one_edition()?;
    let manifest_path = output.join("manifest.json");
    let manifest = read_record(&output, "manifest.json")?;
    let mut legacy = manifest.clone();
    legacy["version"] = json!(1);
    write_json(&manifest_path, &legacy)?;
    refuses(paint(&output, false), "Legacy build");
    write_json(&manifest_path, &manifest)?;
    let file = output.join(path(&manifest["episodes"][0]["file"])?);
    fs::write(file, "Changed!").map_err(io)?;
    refuses(paint(&output, false), "Built subtitle changed");
    assert!(!output.join("renders").exists());
    Ok(())
}

fn ffmpeg(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let executable = renderer::ffmpeg_path(None)?;
    let command = std::iter::once(executable.to_str().ok_or("expected UTF-8 FFmpeg path")?)
        .chain(["-hide_banner", "-nostdin", "-loglevel", "verbose"])
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    run_owned(
        &command,
        &RunOptions {
            cwd: Some(root),
            timeout: Duration::from_secs(20),
            idle_timeout: Some(Duration::from_secs(15)),
            max_output: 4 * 1024 * 1024,
            ..RunOptions::default()
        },
    )
    .map_err(|error| error.to_string())
}

fn ink(pixels: &[u8], width: usize, height: usize) -> (usize, usize, usize, usize) {
    assert_eq!(pixels.len(), width * height * 3);
    assert!(
        pixels.iter().copied().collect::<BTreeSet<_>>().len() > 10,
        "the frame has no visible antialiased text"
    );
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (index, color) in pixels.as_chunks::<3>().0.iter().enumerate() {
        if color.iter().any(|&value| value > 48) {
            let (x, y) = (index % width, index / width);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    assert!(
        0 < left && left < right && right + 1 < width,
        "ink clipped horizontally: {left}..={right}"
    );
    assert!(
        0 < top && top < bottom && bottom + 1 < height,
        "ink clipped vertically: {top}..={bottom}"
    );
    (left, top, right, bottom)
}

fn frame_pixels(output: &Path, frame: &Value) -> Result<Vec<u8>, String> {
    let relative = path(&frame["file"])?;
    let raw = fs::read(output.join(relative)).map_err(io)?;
    let (w, h) = decode_dimensions(&raw)?;
    assert_eq!((w, h), (1280, 720));
    assert_eq!(
        (frame["width"].as_u64(), frame["height"].as_u64()),
        (Some(u64::from(w)), Some(u64::from(h)))
    );
    assert_eq!(frame["sha256"], digest(&raw));
    let decoded = ffmpeg(
        output,
        &[
            "-i",
            relative.to_str().ok_or("expected UTF-8 frame path")?,
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    )?;
    ink(&decoded.stdout, w as usize, h as usize);
    Ok(decoded.stdout)
}

fn oracle(text: &str, bundled: bool) -> Result<(Vec<u8>, String), String> {
    let root = tempfile::tempdir().map_err(io)?;
    fs::write(root.path().join("sample.ass"), text).map_err(io)?;
    let mut filter = "settb=AVTB,setpts=PTS+1.5/TB,ass=sample.ass:shaping=complex".to_owned();
    if bundled {
        let fonts = root.path().join("fonts");
        fs::create_dir(&fonts).map_err(io)?;
        fs::copy(
            renderer::bundled_fonts_dir()
                .ok_or("missing bundled fonts")?
                .join("BKOODB.TTF"),
            fonts.join("BKOODB.TTF"),
        )
        .map_err(io)?;
        filter.push_str(":fontsdir=fonts");
    }
    let output = ffmpeg(
        root.path(),
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=0x202020:s=640x360:r=1:d=1",
            "-filter_threads",
            "1",
            "-vf",
            &filter,
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    )?;
    ink(&output.stdout, 640, 360);
    Ok((
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn same_pixels(expected: &[u8], actual: &[u8]) {
    assert_eq!(expected.len(), actual.len());
    assert_eq!(
        expected.iter().zip(actual).filter(|(a, b)| a != b).count(),
        0,
        "RGB channels changed"
    );
}

#[test]
fn renders_attested_png_from_unicode_and_space_paths() -> Result<(), String> {
    let (_root, _work, output) = one_edition()?;
    assert!(output.to_string_lossy().contains("فضای کار subtitle tests"));
    let evidence = paint(&output, false)?;
    let original = read_record(&output, "renders/S01E01.json")?;
    let receipt = receipt(&output, &evidence)?;
    let first = &evidence["frames"][0];
    let pixels = frame_pixels(&output, first)?;
    assert_eq!(receipt["subtitle_sha256"], episode(&output)?["sha256"]);
    assert_eq!(receipt["frames"][0]["sha256"], first["sha256"]);
    assert_eq!(receipt["recipe"]["profile"], "rgb24-v1");
    assert_eq!(receipt["recipe"]["fonts"][0]["name"], "BKOODB.TTF");
    assert!(
        receipt["recipe"]["requested_fonts"]
            .as_array()
            .ok_or("expected font list")?
            .contains(&json!("B Koodak"))
    );
    refuses(delivery::qa(&output), "Inspect every");
    assert_eq!(read_record(&output, "renders/S01E01.json")?, original);
    inspect(&output)?;
    assert_eq!(delivery::qa(&output)?["reviewed_frames"], 1);
    let second = paint(&output, false)?;
    assert_ne!(first["file"], second["frames"][0]["file"]);
    assert_eq!(first["sha256"], second["frames"][0]["sha256"]);
    same_pixels(&pixels, &frame_pixels(&output, &second["frames"][0])?);
    assert!(
        !fs::read_dir(&output)
            .map_err(io)?
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with(".render-"))
    );
    Ok(())
}

#[test]
fn renders_every_cue_and_attests_the_all_cues_recipe() -> Result<(), String> {
    let text = &ass(
        DEFAULT_STYLE,
        "",
        &(0..4)
            .map(|i| event(i, i + 1, "VISIBLE"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let (_root, work) = workspace(&[("S01E01.ass", text)], "en")?;
    let output = edition(&work)?;
    assert_eq!(
        paint(&output, false)?["frames"]
            .as_array()
            .ok_or("expected frames")?
            .len(),
        2
    );
    let all = paint(&output, true)?;
    let receipt = receipt(&output, &all)?;
    assert_eq!(receipt["all_cues"], true);
    let frames = all["frames"].as_array().ok_or("expected frames")?;
    assert_eq!(frames.len(), 4);
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame["cue_indices"], json!([index + 1]));
        frame_pixels(&output, frame)?;
    }
    Ok(())
}

#[test]
fn renders_literal_srt_delimiters_as_visible_content() -> Result<(), String> {
    let (_root, work) = workspace(&[("S01E01.srt", SRT)], "fa")?;
    let sheet = "worksheets/s0001.json";
    let mut rows = read_record(&work, sheet)?;
    rows[0]["action"] = json!("edit");
    rows[0]["text"] = json!("x < 5 و y > 2 رو نگه دار.");
    write_json(&work.join(sheet), &rows)?;
    let output = edition(&work)?;
    let built = fs::read_to_string(output.join("Sub/S01E01.srt")).map_err(io)?;
    assert!(built.contains("x < 5") && built.contains("y > 2"));
    let evidence = paint(&output, true)?;
    frame_pixels(&output, &evidence["frames"][0])?;
    Ok(())
}

#[test]
fn renders_inline_font_and_reset_only_style_with_verified_font_bytes() -> Result<(), String> {
    let reset = DEFAULT_STYLE.replacen("Default", "ResetOnly", 1);
    let text = ass(
        &format!("{DEFAULT_STYLE}\nStyle: {reset}"),
        "",
        &[
            event(0, 1, "Visible"),
            event(
                1,
                2,
                r"{\rResetOnly\fnB Koodak\fsp0\fe-1}من دیروز OVA رو دیدم؛ اسمش «امید» بود.",
            ),
            event(2, 3, "Visible"),
        ]
        .join("\n"),
    );
    let (root, work) = workspace(&[("S01E01.ass", &text)], "fa")?;
    let output = edition(&work)?;
    let fonts = root.path().join("controlled fonts");
    fs::create_dir(&fonts).map_err(io)?;
    let font = fonts.join("authored.ttf");
    let original = fs::read(
        renderer::bundled_fonts_dir()
            .ok_or("missing bundled fonts")?
            .join("BKOODB.TTF"),
    )
    .map_err(io)?;
    fs::write(&font, &original).map_err(io)?;
    let evidence = renderer::render(&output, EPISODE, None, None, Some(&fonts), false)?;
    let record = receipt(&output, &evidence)?;
    assert_eq!(record["recipe"]["fonts"][0]["name"], "authored.ttf");
    assert_eq!(record["recipe"]["fonts"][0]["sha256"], digest(&original));
    assert_eq!(record["recipe"]["requested_fonts"], json!(["B Koodak"]));
    assert_eq!(fs::read(font).map_err(io)?, original);
    let frames = evidence["frames"].as_array().ok_or("expected frames")?;
    let selected = frames
        .iter()
        .find(|f| f["cue_indices"] == json!([2]))
        .ok_or("reset-only cue was not sampled")?;
    frame_pixels(&output, selected)?;
    Ok(())
}

#[test]
fn renders_fr019_latin_first_persian_isolation_and_preserves_bold_italic() -> Result<(), String> {
    let subtitle = r"{\b1\i1}NASA اینجاست!{\i0\b0}";
    let (_root, work) = workspace(&[("S01E01.ass", &source(subtitle))], "fa")?;
    let output = edition(&work)?;
    let raw = fs::read_to_string(output.join(path(&episode(&output)?["file"])?)).map_err(io)?;
    let expected = "{\\b1\\i1}\u{200f}\u{2067}NASA اینجاست!\u{2069}{\\i0\\b0}";
    assert_eq!(parse(&raw, "ass")?.cues[0].text, expected);
    let evidence = paint(&output, false)?;
    frame_pixels(&output, &evidence["frames"][0])?;
    let (styled, trace) = oracle(&raw, true)?;
    assert!(
        trace.lines().any(|line| line.contains("fontselect:")
            && line.contains("B Koodak")
            && (line.contains("BKOODB") || line.contains("BKoodak"))),
        "bundled Koodak was not selected: {trace}"
    );
    let plain = raw.replace(r"{\b1\i1}", r"{\b0\i0}");
    assert!(
        styled != oracle(&plain, true)?.0,
        "formatting did not change any pixels"
    );
    Ok(())
}

#[test]
fn translator_mode_preserves_source_compositing_pixels() -> Result<(), String> {
    let later = event(1, 3, r"{\pos(160,90)\1c&H0000FF&}VISIBLE");
    let earlier = event(0, 3, r"{\pos(160,90)\1c&H00FF00&}VISIBLE");
    let raw = ass(STYLE, "", &format!("{later}\n{earlier}"));
    let doc = parse(&raw, "ass")?;
    let options = Options {
        style: Some(STYLE),
        direction: "ltr",
        fix_zwnj: false,
        wrap_lines: false,
        strip_watermarks: false,
        ..Options::default()
    };
    let (translated, stats) = conversion::build_translator_ass(&doc, &[], true, &options)?;
    let serialized = serialize(&translated, &translated.cues, false)?;
    let roundtrip = parse(&serialized, "ass")?;
    assert_eq!(
        roundtrip.cues.iter().map(|c| c.start).collect::<Vec<_>>(),
        [1000, 0]
    );
    assert!(
        roundtrip
            .cues
            .iter()
            .zip(&doc.cues)
            .all(|(a, b)| a.text == b.text)
    );
    assert_eq!(stats["placeholder_slots"], 0);
    let baseline = oracle(&raw, false)?.0;
    let swapped = oracle(&ass(STYLE, "", &format!("{earlier}\n{later}")), false)?.0;
    assert!(
        baseline != swapped,
        "fixture must detect reversed compositing"
    );
    same_pixels(&baseline, &oracle(&serialized, false)?.0);
    Ok(())
}

#[test]
fn ass_conversion_keeps_literal_tag_pixels() -> Result<(), String> {
    let raw = ass(STYLE, "", &event(1, 3, "A <i> B <br> C"));
    let doc = parse(&raw, "ass")?;
    let (converted, _) = conversion::ass_document_to_ass(
        &doc,
        &Options {
            style: Some(STYLE),
            direction: "ltr",
            fix_zwnj: false,
            wrap_lines: false,
            strip_watermarks: false,
            ..Options::default()
        },
    )?;
    let result = serialize(&converted, &converted.cues, false)?;
    assert_eq!(parse(&result, "ass")?.cues[0].text, "A <i> B <br> C");
    same_pixels(&oracle(&raw, false)?.0, &oracle(&result, false)?.0);
    Ok(())
}

#[test]
fn native_numeric_prefix_padding_changes_libass_pixels() -> Result<(), String> {
    let plain =
        ass(STYLE, "", &event(1, 3, "AVAVAV To To")).replace("PlayResX: 320", "PlayResX:&H140");
    let padded = plain.replace("PlayResX:&H140", "PlayResX: &H140");
    let a = oracle(&plain, false)?.0;
    let b = oracle(&padded, false)?.0;
    same_pixels(&a, &oracle(&plain.replace("&H140", "320"), false)?.0);
    same_pixels(&b, &oracle(&plain.replace("&H140", "240"), false)?.0);
    assert!(a != b, "libass treated leading padding as cosmetic");
    Ok(())
}

#[test]
fn native_unicode_kerning_padding_changes_libass_pixels() -> Result<(), String> {
    let event = event(1, 3, "AVAVAV To To");
    let style = if cfg!(windows) {
        STYLE.to_owned()
    } else {
        STYLE.replace("Arial", "DejaVu Sans")
    };
    let a = oracle(&ass(&style, "Kerning: yes", &event), false)?.0;
    let b = oracle(&ass(&style, "Kerning: \u{a0}yes", &event), false)?.0;
    assert!(
        a != b,
        "kerning fixture must distinguish unicode padding on native libass"
    );
    Ok(())
}

#[test]
fn rejects_oversized_fonts_without_replacing_the_review() -> Result<(), String> {
    let (root, _work, output) = one_edition()?;
    paint(&output, false)?;
    inspect(&output)?;
    let review = output.join("renders/S01E01.json");
    let previous = fs::read(&review).map_err(io)?;
    let fonts = root.path().join("fonts");
    fs::create_dir(&fonts).map_err(io)?;
    let font = fonts.join("oversized.ttf");
    let size = delivery::MAX_FONT_BYTES + 1;
    File::create(&font)
        .and_then(|f| f.set_len(size))
        .map_err(io)?;
    refuses(
        renderer::render(&output, EPISODE, None, None, Some(&fonts), false),
        "file-size limit",
    );
    assert_eq!(fs::metadata(font).map_err(io)?.len(), size);
    assert_eq!(fs::read(&review).map_err(io)?, previous);
    assert_eq!(delivery::qa(&output)?["ok"], true);
    Ok(())
}

#[test]
fn failed_render_preserves_review_and_retry_requires_fresh_inspection() -> Result<(), String> {
    let (_root, _work, output) = one_edition()?;
    let initial = paint(&output, false)?;
    let record = inspect(&output)?;
    let frame = path(&initial["frames"][0]["file"])?;
    let before = [
        Path::new("renders/S01E01.json"),
        path(&record["receipt_file"])?,
        frame,
    ]
    .map(|p| (output.join(p), fs::read(output.join(p))));
    let subtitle = output.join(path(&episode(&output)?["file"])?);
    let original = fs::read(&subtitle).map_err(io)?;
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&subtitle, changed).map_err(io)?;
    refuses(paint(&output, false), "Built subtitle changed");
    for (file, bytes) in before {
        assert_eq!(fs::read(file).map_err(io)?, bytes.map_err(io)?);
    }
    fs::write(subtitle, original).map_err(io)?;
    assert_eq!(delivery::qa(&output)?["ok"], true);
    let fresh = paint(&output, false)?;
    assert_ne!(fresh["frames"][0]["file"], initial["frames"][0]["file"]);
    assert!(
        fresh["frames"]
            .as_array()
            .ok_or("expected frames")?
            .iter()
            .all(|f| f["reviewed"] == false)
    );
    refuses(delivery::qa(&output), "Inspect every");
    Ok(())
}

#[test]
fn old_sampler_requires_rerender_without_editing_sources_or_worksheets() -> Result<(), String> {
    let (_root, work, output) = one_edition()?;
    let files = [
        work.join("sources/s0001.ass"),
        work.join("worksheets/s0001.json"),
    ];
    let before = files.each_ref().map(fs::read);
    let evidence = paint(&output, false)?;
    let mut receipt = receipt(&output, &evidence)?;
    let mut review = inspect(&output)?;
    let receipt_file = output.join(path(&review["receipt_file"])?);
    receipt["recipe"]["sampler"] = json!(sampling::SAMPLER_VERSION - 1);
    write_json(&receipt_file, &receipt)?;
    review["receipt_sha256"] = json!(digest(&fs::read(receipt_file).map_err(io)?));
    write_json(&output.join("renders/S01E01.json"), &review)?;
    refuses(delivery::qa(&output), "recipe changed");
    assert_eq!(edition(&work)?, output);
    paint(&output, false)?;
    inspect(&output)?;
    assert_eq!(delivery::qa(&output)?["ok"], true);
    for (file, bytes) in files.into_iter().zip(before) {
        assert_eq!(fs::read(file).map_err(io)?, bytes.map_err(io)?);
    }
    Ok(())
}

#[test]
fn rejects_corrupted_frame_and_refuses_to_package_tampered_subtitles() -> Result<(), String> {
    let (root, _work, output) = one_edition()?;
    let evidence = paint(&output, false)?;
    let frame = &evidence["frames"][0];
    frame_pixels(&output, frame)?;
    inspect(&output)?;
    let image = output.join(path(&frame["file"])?);
    let mut damaged = fs::read(&image).map_err(io)?;
    damaged[30] ^= 1;
    fs::write(image, damaged).map_err(io)?;
    refuses(delivery::qa(&output), "Rendered image changed");
    paint(&output, false)?;
    inspect(&output)?;
    let zip = root.path().join("verified.zip");
    let episode = episode(&output)?;
    let subtitle = output.join(path(&episode["file"])?);
    let original = delivery::subtitle_snapshot(&output, &episode)?;
    fs::write(&subtitle, b"changed subtitle").map_err(io)?;
    refuses(delivery::package(&output, &zip), "Built subtitle changed");
    assert!(!zip.exists());
    fs::write(subtitle, &original).map_err(io)?;
    let packaged = delivery::package(&output, &zip)?;
    let archive = fs::read(zip).map_err(io)?;
    assert_eq!(packaged["sha256"], digest(&archive));
    assert_eq!(packaged["episodes"], 1);
    assert_single_zip_entry(
        &archive,
        episode["file"].as_str().ok_or("expected subtitle file")?,
        &original,
    )
    .map_err(io)?;
    Ok(())
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes[..2].try_into().expect("ZIP 16-bit field"))
}

fn le_u32(bytes: &[u8]) -> usize {
    u32::from_le_bytes(bytes[..4].try_into().expect("ZIP 32-bit field")) as usize
}

fn assert_single_zip_entry(archive: &[u8], filename: &str, expected: &[u8]) -> std_io::Result<()> {
    assert_eq!(&archive[..4], b"PK\x03\x04");
    let end = &archive[archive.len() - 22..];
    assert_eq!(
        &end[..4],
        b"PK\x05\x06",
        "fixture archive has no ZIP comment"
    );
    assert_eq!(le_u16(&end[8..]), 1);
    assert_eq!(le_u16(&end[10..]), 1);
    let offset = le_u32(&end[16..]);
    let central = &archive[offset..archive.len() - 22];
    assert_eq!(central.len(), le_u32(&end[12..]));
    assert_eq!(&central[..4], b"PK\x01\x02");
    assert_eq!(le_u32(&central[42..]), 0);
    let central_name = usize::from(le_u16(&central[28..]));
    assert_eq!(&central[46..46 + central_name], filename.as_bytes());
    assert_eq!(le_u32(&central[24..]), expected.len());
    assert_eq!(le_u32(&central[16..]), crc32fast::hash(expected) as usize);
    let local_name = usize::from(le_u16(&archive[26..]));
    assert_eq!(&archive[30..30 + local_name], filename.as_bytes());
    let start = 30 + local_name + usize::from(le_u16(&archive[28..]));
    let payload = &archive[start..start + le_u32(&central[20..])];
    assert_eq!(le_u16(&central[10..]), 8);
    let mut unpacked = Vec::new();
    DeflateDecoder::new(payload)
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut unpacked)?;
    assert_eq!(unpacked, expected);
    Ok(())
}
