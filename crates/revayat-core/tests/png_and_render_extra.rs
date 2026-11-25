use flate2::{Compression, write::ZlibEncoder};
#[cfg(unix)]
use revayat_core::process_control::{RunOptions, run_owned};
use revayat_core::{
    png_validation::{MAX_PNG_BYTES, decode_dimensions},
    render::{delivery, sampling},
    subtitle_formats::document::{
        ASS_FIELDS, DEFAULT_STYLE, Document, STYLE_FIELDS, parse, serialize,
    },
    workflow::{contract::io_error, review::donor, workspace::digest},
};
use serde_json::json;
#[cfg(unix)]
use std::time::Duration;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Write},
};

const SRT: &str = "1\n00:00:01,001 --> 00:00:02,009\nx < 5 and y > 2\n";

fn refuses<T>(result: Result<T, String>, reason: &str) -> String {
    let error = result.err().expect("expected refusal");
    assert!(error.contains(reason), "expected {reason:?}, got {error:?}");
    error
}

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend(
        u32::try_from(body.len())
            .expect("small fixture chunk")
            .to_be_bytes(),
    );
    let start = out.len();
    out.extend(kind);
    out.extend(body);
    out.extend(crc32fast::hash(&out[start..]).to_be_bytes());
}

fn header(width: u32, height: u32, profile: [u8; 5]) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    let mut body = width.to_be_bytes().to_vec();
    body.extend(height.to_be_bytes());
    body.extend(profile);
    chunk(&mut out, b"IHDR", &body);
    out
}

fn compress(pixels: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(pixels)?;
    encoder.finish()
}

fn assemble(mut out: Vec<u8>, stream: &[u8]) -> Vec<u8> {
    let (first, second) = stream.split_at(stream.len() / 2);
    chunk(&mut out, b"IDAT", first);
    chunk(&mut out, b"IDAT", second);
    chunk(&mut out, b"IEND", b"");
    out
}

fn image(color: u8, filters: [u8; 2]) -> io::Result<Vec<u8>> {
    let stride = if color == 2 { 9 } else { 12 };
    let raw: Vec<_> = filters
        .into_iter()
        .flat_map(|filter| [filter].into_iter().chain(0..stride))
        .collect();
    Ok(assemble(
        header(3, 2, [8, color, 0, 0, 0]),
        &compress(&raw)?,
    ))
}

fn png_refuses(data: &[u8], reason: &str) {
    let error = decode_dimensions(data).expect_err("invalid PNG was accepted");
    assert!(error.contains(reason), "expected {reason:?}, got {error:?}");
}

#[test]
fn rgb_accepts_every_filter_on_first_and_following_rows() -> io::Result<()> {
    for filter in 0..=4 {
        assert_eq!(decode_dimensions(&image(2, [filter; 2])?), Ok((3, 2)));
    }
    Ok(())
}

#[test]
fn rgba_accepts_every_filter_on_first_and_following_rows() -> io::Result<()> {
    for filter in 0..=4 {
        assert_eq!(decode_dimensions(&image(6, [filter; 2])?), Ok((3, 2)));
    }
    Ok(())
}

#[test]
fn rejects_invalid_scanline_filter_on_both_rows() -> io::Result<()> {
    for filter in [5, 255] {
        png_refuses(&image(2, [filter, 0])?, "filter");
        png_refuses(&image(6, [0, filter])?, "filter");
    }
    Ok(())
}

#[test]
fn rejects_short_and_oversized_decompressed_streams() -> io::Result<()> {
    for size in [0, 19, 21] {
        let data = assemble(header(3, 2, [8, 2, 0, 0, 0]), &compress(&vec![0; size])?);
        png_refuses(&data, "stream");
    }
    Ok(())
}

#[test]
fn rejects_truncated_trailing_and_concatenated_zlib_members() -> io::Result<()> {
    let valid = compress(&[0; 20])?;
    for stream in [
        &valid[..valid.len() - 1],
        &[valid.as_slice(), b"tail"].concat(),
        &[valid.as_slice(), &compress(b"")?].concat(),
    ] {
        png_refuses(&assemble(header(3, 2, [8, 2, 0, 0, 0]), stream), "stream");
    }
    let mut bad_adler = valid;
    let last = bad_adler.len() - 1;
    bad_adler[last] ^= 1;
    png_refuses(
        &assemble(header(3, 2, [8, 2, 0, 0, 0]), &bad_adler),
        "compressed image data",
    );
    Ok(())
}

#[test]
fn rejects_bad_crc_even_on_empty_or_ancillary_chunks() -> io::Result<()> {
    let mut data = image(2, [0; 2])?;
    *data.last_mut().expect("IEND CRC") ^= 1;
    png_refuses(&data, "CRC");
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut out, b"tEXt", b"author\0fixture");
    *out.last_mut().expect("ancillary CRC") ^= 1;
    png_refuses(&assemble(out, &compress(&[0; 20])?), "CRC");
    Ok(())
}

#[test]
fn rejects_png_profiles_other_than_noninterlaced_eight_bit_rgb_or_rgba() -> io::Result<()> {
    for profile in [
        [16, 2, 0, 0, 0],
        [8, 2, 0, 0, 1],
        [8, 0, 0, 0, 0],
        [8, 3, 0, 0, 0],
        [8, 2, 1, 0, 0],
        [8, 2, 0, 1, 0],
    ] {
        png_refuses(
            &assemble(header(3, 2, profile), &compress(&[0; 20])?),
            "RGB/RGBA",
        );
    }
    Ok(())
}

#[test]
fn enforces_dimension_and_total_pixel_caps() {
    for (width, height) in [(0, 2), (3, 0), (8193, 1), (1, 8193), (8192, 2049)] {
        png_refuses(&header(width, height, [8, 2, 0, 0, 0]), "dimensions");
    }
}

#[test]
fn enforces_png_file_cap_before_chunk_processing() -> io::Result<()> {
    let mut large = image(2, [0; 2])?;
    large.resize(MAX_PNG_BYTES + 1, 0);
    png_refuses(&large, "file size");
    Ok(())
}

#[test]
fn rejects_corrupt_signature_and_truncated_chunk_headers_or_bodies() -> io::Result<()> {
    let data = image(2, [0; 2])?;
    png_refuses(&data[1..], "signature");
    png_refuses(&data[..17], "truncated chunk");
    png_refuses(&data[..24], "truncated chunk body");
    Ok(())
}

#[test]
fn requires_header_first_and_only_once() {
    let mut out = SIGNATURE.to_vec();
    chunk(&mut out, b"IDAT", &[]);
    png_refuses(&out, "chunk sequence");
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    let another = out[SIGNATURE.len()..].to_vec();
    out.extend(another);
    png_refuses(&out, "repeated header");
}

#[test]
fn rejects_nonconsecutive_image_data_chunks() -> io::Result<()> {
    let stream = compress(&[0; 20])?;
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut out, b"IDAT", &stream[..3]);
    chunk(&mut out, b"tEXt", b"fixture\0not consecutive");
    chunk(&mut out, b"IDAT", &stream[3..]);
    chunk(&mut out, b"IEND", &[]);
    png_refuses(&out, "noncontiguous image data");
    Ok(())
}

#[test]
fn requires_a_final_empty_iend_after_image_data() -> io::Result<()> {
    let mut valid = image(2, [0; 2])?;
    png_refuses(
        &valid[..valid.len() - 12],
        "missing header, image data or end marker",
    );
    valid.extend(b"tail");
    png_refuses(&valid, "trailing");
    let mut no_data = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut no_data, b"IEND", b"");
    png_refuses(&no_data, "end marker");
    let mut nonempty = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut nonempty, b"IDAT", &compress(&[0; 20])?);
    chunk(&mut nonempty, b"IEND", b"tail");
    png_refuses(&nonempty, "end marker");
    Ok(())
}

#[test]
fn distinguishes_skippable_ancillary_chunks_from_invalid_and_critical_chunks() -> io::Result<()> {
    for (kind, reason) in [
        (b"TEXT", "unsupported critical"),
        (b"text", "chunk type"),
        (b"tE0t", "chunk type"),
    ] {
        let mut out = header(3, 2, [8, 2, 0, 0, 0]);
        chunk(&mut out, kind, b"author\0fixture");
        png_refuses(&out, reason);
    }
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut out, b"tEXt", b"author\0fixture");
    assert_eq!(
        decode_dimensions(&assemble(out, &compress(&[0; 20])?)),
        Ok((3, 2))
    );
    Ok(())
}

#[test]
fn rejects_animation_chunks_for_single_frame_evidence() {
    for kind in [b"acTL", b"fcTL", b"fdAT"] {
        let mut out = header(3, 2, [8, 2, 0, 0, 0]);
        chunk(&mut out, kind, &[]);
        png_refuses(&out, "animated images");
    }
}

#[test]
fn enforces_chunk_count_budget() {
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    for _ in 0..4096 {
        chunk(&mut out, b"tEXt", b"x\0");
    }
    png_refuses(&out, "excessive chunks");
}

#[test]
fn validates_palette_geometry_uniqueness_and_order() -> io::Result<()> {
    for body in [&[][..], &[0, 1], &[0; 771]] {
        let mut out = header(3, 2, [8, 2, 0, 0, 0]);
        chunk(&mut out, b"PLTE", body);
        png_refuses(&out, "palette");
    }
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut out, b"PLTE", &[0, 1, 2]);
    assert_eq!(
        decode_dimensions(&assemble(out.clone(), &compress(&[0; 20])?)),
        Ok((3, 2))
    );
    chunk(&mut out, b"PLTE", &[0, 1, 2]);
    png_refuses(&out, "palette");
    let mut out = header(3, 2, [8, 2, 0, 0, 0]);
    chunk(&mut out, b"IDAT", &compress(&[0; 20])?);
    chunk(&mut out, b"PLTE", &[0, 1, 2]);
    png_refuses(&out, "palette");
    Ok(())
}

fn track(headers: &str) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n{headers}\n[V4+ Styles]\nFormat: {STYLE_FIELDS}\nStyle: {DEFAULT_STYLE}\n[Events]\nFormat: {ASS_FIELDS}\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,VISIBLE\n"
    )
}

fn field(label: &str, after_colon: &str) -> Result<Document, String> {
    let line = format!("{label}:{after_colon}");
    let raw = match label {
        "PlayResX" => track("").replace("PlayResX: 320", &line),
        "PlayResY" => track("").replace("PlayResY: 180", &line),
        _ => track(&line),
    };
    parse(&raw, "ass")
}

fn merge_refuses(base: &mut Document, incoming: &Document, reason: &str) {
    let before = (base.clone(), incoming.clone());
    let error = donor(base, incoming, incoming.cues.clone(), "s0002", true)
        .expect_err("incompatible track merged");
    assert!(error.contains(reason), "expected {reason:?}, got {error:?}");
    assert_eq!((base.clone(), incoming.clone()), before);
}

#[test]
fn rejects_leading_padding_on_hexadecimal_track_integers_atomically() -> Result<(), String> {
    for label in [
        "PlayResX",
        "PlayResY",
        "LayoutResX",
        "LayoutResY",
        "WrapStyle",
    ] {
        for token in ["&H140", "&h140", "0x140", "0X140"] {
            for padding in [" ", "\t", " \t"] {
                let mut base = field(label, token)?;
                let incompatible = field(label, &format!("{padding}{token}"))?;
                merge_refuses(&mut base, &incompatible, "hexadecimal");
                let before = base.clone();
                assert!(donor(&mut base, &incompatible, vec![], "s0002", true)?.is_empty());
                assert_eq!(base, before);
                let compatible = field(label, token)?;
                assert_eq!(
                    donor(
                        &mut base,
                        &compatible,
                        compatible.cues.clone(),
                        "s0002",
                        true
                    )?
                    .len(),
                    1
                );
            }
        }
    }
    Ok(())
}

#[test]
fn decimal_padding_and_hexadecimal_suffix_padding_still_merge() -> Result<(), String> {
    for (first, second) in [
        ("320", " \t320\t "),
        ("&H140", "&H140\t "),
        ("0x140", "0x140 "),
    ] {
        let mut base = field("PlayResX", first)?;
        let incoming = field("PlayResX", second)?;
        assert_eq!(
            donor(&mut base, &incoming, incoming.cues.clone(), "s0002", true)?.len(),
            1
        );
    }
    Ok(())
}

#[test]
fn unicode_whitespace_does_not_alias_track_values_in_either_order() -> Result<(), String> {
    let cases = [
        ("PlayResX", "320"),
        ("PlayResY", "180"),
        ("LayoutResX", "320"),
        ("LayoutResY", "180"),
        ("WrapStyle", "2"),
        ("ScaledBorderAndShadow", "yes"),
        ("Kerning", "yes"),
        ("YCbCr Matrix", "TV.709"),
        ("Language", "fa"),
        ("Collisions", "Normal"),
        ("Timer", "100"),
    ];
    let mut cases_checked = 0;
    for (label, value) in cases {
        for separator in [
            '\u{a0}', '\u{1680}', '\u{2000}', '\u{2007}', '\u{202f}', '\u{205f}', '\u{3000}',
        ] {
            let padded = format!("{separator}{value}");
            for (first, second) in [(value, padded.as_str()), (padded.as_str(), value)] {
                merge_refuses(
                    &mut field(label, &format!(" {first}"))?,
                    &field(label, &format!(" {second}"))?,
                    "track settings",
                );
                cases_checked += 1;
            }
        }
    }
    assert_eq!(cases_checked, 154);
    Ok(())
}

#[test]
fn ascii_track_padding_keeps_donor_style_namespace() -> Result<(), String> {
    for (first, second) in [(" yes", "  \tyes\t "), (" \tyes ", " yes")] {
        let mut base = field("Kerning", first)?;
        let incoming = field("Kerning", second)?;
        let merged = donor(&mut base, &incoming, incoming.cues.clone(), "s0002", true)?;
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].fields["style"], "s0002__Default");
    }
    Ok(())
}

#[test]
fn equal_unicode_values_merge_and_preserve_original_header_bytes() -> Result<(), String> {
    let mut base = field("Kerning", " \u{a0}yes")?;
    let incoming = base.clone();
    let merged = donor(&mut base, &incoming, incoming.cues.clone(), "s0002", true)?;
    let mut cues = base.cues.clone();
    cues.extend(merged);
    assert_eq!(cues.len(), 2);
    assert!(serialize(&base, &cues, false)?.contains("Kerning: \u{a0}yes"));
    Ok(())
}

#[test]
fn last_duplicate_header_value_remains_effective() -> Result<(), String> {
    let mut base = parse(&track("Kerning: no\nKerning: yes"), "ass")?;
    let incoming = field("Kerning", " yes")?;
    assert_eq!(
        donor(&mut base, &incoming, incoming.cues.clone(), "s0002", true)?.len(),
        1
    );
    Ok(())
}

#[test]
fn empty_donor_does_not_impose_unused_track_settings() -> Result<(), String> {
    let mut base = field("Kerning", " yes")?;
    let incoming = field("Kerning", " \u{a0}yes")?;
    merge_refuses(&mut base, &incoming, "track settings");
    let before = base.clone();
    assert!(donor(&mut base, &incoming, vec![], "s0002", true)?.is_empty());
    assert_eq!(base, before);
    Ok(())
}

#[test]
fn rejects_noncanonical_track_header_spelling() -> Result<(), String> {
    let mut base = field("Kerning", " yes")?;
    merge_refuses(&mut base, &field("kerning", " yes")?, "canonical");
    Ok(())
}

fn selected(doc: &Document, all: bool, changed: &[i64]) -> Result<BTreeSet<i64>, String> {
    let samples = sampling::samples(doc, all, changed)?;
    let mut selected = BTreeSet::new();
    let mut previous = -1.0;
    for sample in samples {
        let time = sample["time_seconds"].as_f64().ok_or("expected seconds")?;
        assert!(time > previous, "duplicate or out-of-order sample times");
        previous = time;
        for index in sample["cue_indices"].as_array().ok_or("expected indices")? {
            let index = index.as_i64().ok_or("expected integer index")?;
            let cue = &doc.cues[usize::try_from(index - 1).map_err(|error| error.to_string())?];
            assert!((cue.start as f64 / 1000.0..cue.end as f64 / 1000.0).contains(&time));
            selected.insert(index);
        }
    }
    Ok(selected)
}

fn sampling_ass(style: &str, headers: &str, events: &str) -> String {
    track(headers)
        .replace(DEFAULT_STYLE, style)
        .replace(&sampling_event(1, 3, "VISIBLE"), events)
}

fn sampling_event(start: u32, end: u32, text: &str) -> String {
    format!("Dialogue: 0,0:00:{start:02}.00,0:00:{end:02}.00,Default,,0,0,0,,{text}")
}

fn snapshot_source(text: &str) -> String {
    sampling_ass(DEFAULT_STYLE, "", &sampling_event(1, 3, text))
}

fn timeline(intervals: &[(u32, u32)]) -> Result<Document, String> {
    let events = intervals
        .iter()
        .map(|&(start, end)| sampling_event(start, end, "VISIBLE"))
        .collect::<Vec<_>>()
        .join("\n");
    parse(&sampling_ass(DEFAULT_STYLE, "", &events), "ass")
}

#[test]
fn samples_temporal_edges_in_unsorted_overlap() -> Result<(), String> {
    let doc = timeline(&[(3, 10), (0, 10), (9, 10), (2, 10)])?;
    assert_eq!(selected(&doc, false, &[])?, BTreeSet::from([1, 2, 3, 4]));
    Ok(())
}

#[test]
fn samples_longest_tail_and_changed_cues() -> Result<(), String> {
    let doc = timeline(&[(3, 5), (1, 12), (2, 4), (0, 4)])?;
    let changes = sampling::changed_indices(
        &json!({"provenance":[{"structure_changed":false,"emitted_index":1},{"structure_changed":true,"emitted_index":3}]}),
    )?;
    assert_eq!(changes, [3]);
    assert_eq!(selected(&doc, false, &[])?, BTreeSet::from([1, 2, 4]));
    assert_eq!(
        selected(&doc, false, &changes)?,
        BTreeSet::from([1, 2, 3, 4])
    );
    Ok(())
}

#[test]
fn samples_every_cue_only_on_request_and_deduplicates_times() -> Result<(), String> {
    let doc = timeline(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)])?;
    assert_eq!(selected(&doc, false, &[])?, BTreeSet::from([1, 5]));
    assert_eq!(selected(&doc, true, &[])?, BTreeSet::from([1, 2, 3, 4, 5]));
    assert_eq!(
        sampling::samples(&timeline(&[(1, 3), (1, 3)])?, true, &[])?,
        [json!({"time_seconds":2.0,"cue_indices":[1,2]})]
    );
    Ok(())
}

#[test]
fn samples_animations_at_three_interior_times() -> Result<(), String> {
    let doc = parse(&snapshot_source(r"{\move(40,50,240,50)}VISIBLE"), "ass")?;
    let samples = sampling::samples(&doc, false, &[])?;
    assert_eq!(
        samples
            .iter()
            .map(|s| s["time_seconds"].clone())
            .collect::<Vec<_>>(),
        [json!(1.2), json!(2.0), json!(2.8)]
    );
    Ok(())
}

#[test]
fn rejects_missing_or_out_of_range_sample_inputs() -> Result<(), String> {
    refuses(
        sampling::samples(&Document::default(), false, &[]),
        "at least one cue",
    );
    let doc = timeline(&[(1, 3)])?;
    for index in [0, 2, i64::MIN] {
        refuses(
            sampling::samples(&doc, false, &[index]),
            "outside the episode",
        );
    }
    Ok(())
}

#[test]
fn samples_reset_styles_mixed_text_and_tracks_inline_fonts() -> Result<(), String> {
    let mut doc = timeline(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)])?;
    let reset = DEFAULT_STYLE.replace("Default,B Koodak", "ResetOnly,Arial");
    let template = parse(
        &sampling_ass(
            &format!("{DEFAULT_STYLE}\nStyle: {reset}"),
            "",
            &sampling_event(0, 1, "text"),
        ),
        "ass",
    )?;
    doc.styles = template.styles;
    doc.cues[1].text = r"{\rResetOnly\fnDejaVu Sans\fsp0\fe-1}VISIBLE".into();
    doc.cues[2].text = "OVA «امید»".into();
    assert_eq!(selected(&doc, false, &[])?, BTreeSet::from([1, 2, 3, 5]));
    assert_eq!(
        sampling::requested_fonts(&doc)?,
        ["Arial", "B Koodak", "DejaVu Sans"]
    );
    Ok(())
}

#[test]
fn accepts_exact_ass_and_srt_snapshot_bytes() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io_error)?;
    fs::create_dir(root.path().join("Sub")).map_err(io_error)?;
    for (kind, text) in [
        ("ass", snapshot_source("سلام OVA")),
        ("srt", SRT.replace("x < 5 and y > 2", "سلام OVA")),
    ] {
        let relative = format!("Sub/S01E01.{kind}");
        fs::write(root.path().join(&relative), &text).map_err(io_error)?;
        assert_eq!(
            delivery::subtitle_snapshot(
                root.path(),
                &json!({"file":relative,"sha256":digest(text.as_bytes())})
            )?,
            text.as_bytes()
        );
    }
    Ok(())
}

#[test]
fn refuses_snapshot_hash_drift_without_echoing_captions() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io_error)?;
    let original = root.path().join("episode.ass");
    let episode = json!({"file":"episode.ass","sha256":digest(track("").as_bytes())});
    let caption = "private-sentinel-caption";
    fs::write(&original, caption).map_err(io_error)?;
    let error = refuses(
        delivery::subtitle_snapshot(root.path(), &episode),
        "snapshot",
    );
    assert!(!error.contains(caption));
    assert_eq!(fs::read_to_string(original).map_err(io_error)?, caption);
    Ok(())
}

#[test]
fn enforces_snapshot_byte_limit_without_modifying_input() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io_error)?;
    let input = root.path().join("episode.ass");
    let size = 64 * 1024 * 1024 + 1;
    File::create(&input)
        .and_then(|file| file.set_len(size))
        .map_err(io_error)?;
    refuses(
        delivery::subtitle_snapshot(
            root.path(),
            &json!({"file":"episode.ass","sha256":"not consulted"}),
        ),
        "byte limit",
    );
    assert_eq!(fs::metadata(input).map_err(io_error)?.len(), size);
    Ok(())
}

#[test]
fn rejects_non_regular_snapshot_directory_and_escape() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io_error)?;
    fs::create_dir(root.path().join("episode.ass")).map_err(io_error)?;
    refuses(
        delivery::subtitle_snapshot(
            root.path(),
            &json!({"file":"episode.ass","sha256":"unused"}),
        ),
        "regular",
    );
    refuses(
        delivery::subtitle_snapshot(
            root.path(),
            &json!({"file":"../episode.ass","sha256":"unused"}),
        ),
        "canonical",
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn renders_with_a_symlinked_video() -> Result<(), String> {
    use revayat_core::{
        render::renderer,
        runtime::files::write_json,
        workflow::{
            import::{ImportOptions, prepare},
            workspace::{build, read_record},
        },
    };
    use std::{os::unix::fs::symlink, path::Path};

    let root = tempfile::tempdir().map_err(io_error)?;
    let source = root.path().join("S01E01.srt");
    fs::write(
        &source,
        "1\n00:00:00,100 --> 00:00:00,900\nVisible subtitle.\n",
    )
    .map_err(io_error)?;
    let work = root.path().join("work");
    prepare(
        &[source],
        &work,
        &ImportOptions {
            series: "Symlink render fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )?;
    let mut project = read_record(&work, "project.json")?;
    project["episodes"] =
        json!([{"id":"S01E01","base":"s0001","alternates":[],"comparison":"Single source"}]);
    write_json(&work.join("project.json"), &project)?;
    write_json(
        &work.join("glossary.json"),
        &json!({"series":"Symlink render fixture",
        "terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored fixture"}]}),
    )?;
    let sheet = "worksheets/s0001.json";
    let mut rows = read_record(&work, sheet)?;
    rows[0]["action"] = json!("preserve");
    rows[0]["reviewed"] = json!(true);
    write_json(&work.join(sheet), &rows)?;
    let built = build(&work, None)?;
    let edition = Path::new(built["build"].as_str().ok_or("Missing build path")?);

    let video = root.path().join("real video.y4m");
    let mut clip = b"YUV4MPEG2 W32 H32 F10:1 Ip A1:1 C420jpeg\n".to_vec();
    for _ in 0..15 {
        clip.extend_from_slice(b"FRAME\n");
        clip.extend_from_slice(&[128; 32 * 32 * 3 / 2]);
    }
    fs::write(&video, &clip).map_err(io_error)?;
    let link = root.path().join("selected video.y4m");
    symlink(&video, &link).map_err(io_error)?;
    let receipt = renderer::render(edition, "S01E01", None, Some(&link), None, true)?;
    assert!(
        receipt["frames"]
            .as_array()
            .is_some_and(|frames| !frames.is_empty())
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_fifo_snapshot_before_reading() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(io_error)?;
    run_owned(
        &["mkfifo".into(), "episode.ass".into()],
        &RunOptions {
            cwd: Some(root.path()),
            timeout: Duration::from_secs(5),
            ..RunOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    refuses(
        delivery::subtitle_snapshot(
            root.path(),
            &json!({"file":"episode.ass","sha256":"unused"}),
        ),
        "regular",
    );
    Ok(())
}
