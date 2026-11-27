use revayat_core::{
    markup,
    publication::publish_bytes,
    render::{delivery, sampling},
    runtime::{assets, files},
    subtitle_formats::{
        conversion::{self, Options},
        document::{self, Cue, Document},
        visible,
    },
    text_pipeline as text,
    workflow::{
        import::{self, ImportOptions},
        review, workspace,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

type Check = Result<(), String>;
const SRT: &str = "1\n00:00:01,001 --> 00:00:02,009\n<i>می روم.</i>\n\n2\n00:00:04,000 --> 00:00:05,000\nSubtitles by A\n";
const RLE: char = '\u{202b}';
const RLM: char = '\u{200f}';
const PDF: char = '\u{202c}';
const RTL: &str = "\u{200f}\u{2067}";
const PDI: char = '\u{2069}';

fn err<T: std::fmt::Debug>(result: Result<T, String>, expected: &str) {
    let error = result.expect_err(expected);
    assert!(
        error.contains(expected),
        "{error:?} must contain {expected:?}"
    );
}
fn write(path: &Path, data: impl AsRef<[u8]>) -> Check {
    fs::write(path, data).map_err(|e| e.to_string())
}
fn prepare(input: &Path, work: &Path) -> Result<Value, String> {
    import::prepare(
        &[input.to_owned()],
        work,
        &ImportOptions {
            series: "Episode",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "fa",
            output_format: "ass",
        },
    )
}
fn reviewed(root: &Path) -> Result<PathBuf, String> {
    let input = root.join("input.srt");
    let work = root.join("review work");
    write(&input, SRT)?;
    prepare(&input, &work)?;
    let mut project = workspace::read_record(&work, "project.json")?;
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","comparison":"Only release"}]);
    files::write_json(&work.join("project.json"), &project)?;
    let sheet = "worksheets/s0001.json";
    let mut rows = workspace::read_record(&work, sheet)?;
    for row in rows.as_array_mut().ok_or("worksheet array")? {
        row["reviewed"] = json!(true);
        row["action"] = json!("preserve");
    }
    files::write_json(&work.join(sheet), &rows)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({"series":"Episode","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.invalid/episode","note":"Fixture series names"}]}),
    )?;
    Ok(work)
}
fn built_dir(result: &Value) -> Result<PathBuf, String> {
    Ok(PathBuf::from(result["build"].as_str().ok_or("build path")?))
}

macro_rules! zwnj {
    ($($name:ident: ($source:literal,$expected:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let expected=$expected.replace('~',"\u{200c}");
            assert_eq!(text::normalize_persian_zwnj($source.into())?,expected);
            assert_eq!(text::normalize_persian_zwnj(expected.clone())?,expected);
            Ok(())
        }
    )+};
}
zwnj! {
    verbs_take_zwnj: ("می روم، نمی توانم؛ می شود، می کند، می خورم، می گویم، نمی دانم، می آید","می~روم، نمی~توانم؛ می~شود، می~کند، می~خورم، می~گویم، نمی~دانم، می~آید"),
    plurals_and_endings_take_zwnj: ("آن ها و این هایش؛ حلقه ها، گل هایی، کتاب هایمان و کتاب هایتان و خانه هایشان","آن~ها و این~هایش؛ حلقه~ها، گل~هایی، کتاب~هایمان و کتاب~هایتان و خانه~هایشان"),
    comparatives_take_zwnj: ("بزرگ تر، سریع ترین، بهترین، تر","بزرگ~تر، سریع~ترین، بهترین، تر"),
    enclitics_take_zwnj: ("رفته ام، خانه ای، گفته اش، ديده اند","رفته~ام، خانه~ای، گفته~اش، دیده~اند"),
    compound_word_spacing: ("هم چنین، بی شک، هیچ وقت، یک دیگر، به جای","همچنین، بی~شک، هیچ~وقت، یکدیگر، به~جای"),
    arabic_letters_normalize_outside_markup: ("مي روم و كتاب را به مصطفى و فاطمة می دهم","می~روم و کتاب را به مصطفی و فاطمه می~دهم"),
    lexical_uses_of_mey_do_not_join: ("می و جام؛ می، و گل؛ می روی میز است","می و جام؛ می، و گل؛ می روی میز است"),
    font_attributes_and_url_stay_literal: (r#"{\fnفونت عربي}<i title="عربي">مي روم</i>\N\n Latin https://example.org/كتاب"#,r#"{\fnفونت عربي}<i title="عربي">می~روم</i>\N\n Latin https://example.org/كتاب"#),
}

macro_rules! ads {
    ($expected:literal; $($name:ident: $sample:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let body=$sample.replace('~',"\u{200c}");
            assert_eq!(text::is_watermark_cue(body.clone())?,$expected);
            assert_eq!(!text::watermark_reason(body)?.is_empty(),$expected);
            Ok(())
        }
    )+};
}
ads!(true;
    persian_subtitle_credit: "زیرنویس از دوست",
    arabic_script_subtitle_credit: "زيرنويس از مترجم",
    translation_and_editing_credit: "ترجمه و تنظیم",
    editing_and_translation_credit: "تنظیم و ترجمه",
    translator_credit: "مترجم: الف",
    dedication_credit: "تقدیم به نگاه شما",
    telegram_channel_credit: "کانال تلگرام",
    subtitle_provider_persian: "ساب~تایتل",
    farsisub_persian: "فارس~ساب",
    movieco_provider: "موویکو",
    digimovies_provider: "دیجی~موویز",
    zarfilm_provider: "زارفیل",
    softsub_provider: "سافت~ساب",
    app_ad_persian: "اپلیکیشن",
    website_ad_persian: "وب~سایت",
    shine_provider: "shine021",
    farsi_subtitle_provider: "farsisubtitle",
    world_subtitle_provider: "worldsubtitle",
    subf2m_provider: "subf2m",
    addic7ed_provider: "addic7ed",
    opensubtitles_provider: "opensubtitles",
    subscene_provider: "subscene",
    yify_provider: "yify",
    yts_provider: "yts",
    sync_credit: "sync and corrected by A",
    synced_credit: "synced and corrected by A",
    subtitle_credit: "subtitles by A",
    caption_credit: "captioned by A",
    encoding_credit: "encoded by A",
    download_credit: "Downloaded from https://example.test",
    vip_ad: "Support us and become VIP",
    product_ad: "Advertise your product here",
    www_ad: "www.example.test",
    http_ad: "http://example.test",
    https_ad: "https://example.test",
    telegram_url_ad: "t.me/channel",
    telegram_handle_ad: "@test_channel",
    studio_name: "Deluxe Media",
    french_subtitle_credit: "Sous-titres : Alice",
    french_alt_subtitle_credit: "Sous-titrage : Alice",
    french_supervision_credit: "Supervision créative : Alice",
    dutch_translation_credit: "Vertaling: Bob",
    dutch_supervision_credit: "Creatief supervisor: Bob",
    japanese_subtitle_credit: "日本語字幕　山田",
    formatted_bidi_credit: "\u{202b}<i> زیرنویس از ما </i>\u{202c}",
);
ads!(false;
    king_dialogue: "king",
    take_ring_dialogue: "Take the ring.",
    help_dialogue: "Hello, help me!",
    king_salute_dialogue: "Long live the king!",
    take_ring_persian_dialogue: "حلقه را بگیر!",
    going_home_dialogue: "حالا می روم خانه.",
    app_in_spoken_dialogue: "گفت این اپلیکیشن را پاک کنم.",
    email_in_spoken_dialogue: "Send mail to a@example.test.",
    ordinary_addicted_dialogue: "He is addicted to it.",
);

#[test]
fn convert_markup_preserves_color_alignment_breaks_and_controls() -> Check {
    let input = r##"{an8}<i><b>A</b></i><br><u>B</u>"##.to_owned()
        + "\n"
        + r##"<s>C</s><font color="#1200F3">D</font>{\an3}"##;
    let expected = r"{\an8}{\i1}{\b1}A{\b0}{\i0}\N{\u1}B{\u0}\N{\s1}C{\s0}{\c&HF30012&}D{\c}{\an3}";
    assert_eq!(text::convert_srt_markup_to_ass(input)?, expected);
    assert_eq!(text::convert_srt_markup_to_ass(expected.into())?, expected);
    let literal = r"{\fnKeep <i> literal}x";
    assert_eq!(text::convert_srt_markup_to_ass(literal.into())?, literal);
    Ok(())
}

#[test]
fn wrapping_balances_prose_without_splitting_words_tags_or_speakers() -> Check {
    let wrap = |input: &str, max| text::wrap_subtitle_lines(input.into(), Some(max), Some(2));
    let input = r"{\i1}one two three four five six seven eight nine ten eleven twelve{\i0}";
    let wrapped = wrap(input, 42)?;
    assert_eq!(
        wrapped,
        r"{\i1}one two three four five six seven\Neight nine ten eleven twelve{\i0}"
    );
    assert_eq!(wrap(&wrapped, 42)?, wrapped);
    let persian = wrap("این خانه‌ها زیباست", 12)?;
    assert!(persian.contains("خانه‌ها") && persian.contains(r"\N"));
    for input in [r"- first speaker\N- second speaker", r"one\Ntwo"] {
        assert_eq!(wrap(input, 42)?, input);
    }
    let split = wrap(
        r"- first speech with more words to wrap\N- second voice",
        20,
    )?;
    assert!(split.starts_with("- first speech") && split.ends_with(r"\N- second voice"));
    Ok(())
}

macro_rules! anchors {
    ($($name:ident: $ending:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let end=$ending;
            let source=format!(r"{{\an8}}- سلام{end}\N{{\i1}}دنیا{end}{{\i0}}");
            let expected=format!(r"{{\an8}}{RTL}- سلام{end}{PDI}\N{{\i1}}{RTL}دنیا{end}{PDI}{{\i0}}");
            let anchor=|body| text::anchor_rtl_line(body,Some("ass".into()),Some("rtl".into()));
            let result=anchor(source)?;
            assert_eq!(result,expected);
            assert_eq!(anchor(result.clone())?,result);
            markup::validate_bidi(result,"ass".into())
        }
    )+};
}
anchors! {
    anchor_period: '.', anchor_persian_comma: '،', anchor_persian_question: '؟', anchor_exclamation: '!',
    anchor_ellipsis: '…', anchor_colon: ':', anchor_persian_semicolon: '؛', anchor_guillemet: '»',
    anchor_paren: ')', anchor_bracket: ']', anchor_double_quote: '"', anchor_single_quote: '\'', anchor_hyphen: '-',
}

#[test]
fn anchor_strips_outer_noise_but_keeps_drawings_and_ltr() -> Check {
    let anchor = |body: String, kind: &str, dir: &str| {
        text::anchor_rtl_line(body, Some(kind.into()), Some(dir.into()))
    };
    assert_eq!(
        anchor(format!("{PDF}{RLM}سلام!{RLE}"), "srt", "rtl")?,
        format!("{RTL}سلام!{PDI}")
    );
    assert_eq!(
        anchor("سلام.\nدنیا!".into(), "srt", "rtl")?,
        format!("{RTL}سلام.{PDI}\n{RTL}دنیا!{PDI}")
    );
    assert_eq!(anchor("سلام!".into(), "srt", "ltr")?, "سلام!");
    assert_eq!(anchor("Hello!".into(), "ass", "rtl")?, "Hello!");
    let drawing = r"{\p1}m 0 0 l 1 2{\p0}";
    assert_eq!(anchor(drawing.into(), "ass", "rtl")?, drawing);
    assert!(
        markup::rtl("سلام.".into(), "ass".into(), "rtl".into(), false)?
            .ends_with(&format!(".{PDI}"))
    );
    Ok(())
}

#[test]
fn process_applies_flags_and_distinct_line_models() -> Check {
    let result = text::process_cue_text(
        "<i>مي روم؟</i>".into(),
        Some("ass".into()),
        Some("rtl".into()),
        Some(42),
        Some(false),
        Some(false),
    )?;
    assert_eq!(result, format!(r"{{\i1}}{RTL}مي روم؟{PDI}{{\i0}}"));
    assert_eq!(
        text::process_cue_text(
            "one two three four five".into(),
            Some("srt".into()),
            Some("ltr".into()),
            Some(14),
            Some(true),
            Some(true)
        )?,
        "one two three\nfour five"
    );
    assert_eq!(
        text::process_cue_text(
            "مي روم".into(),
            Some("srt".into()),
            Some("ltr".into()),
            Some(42),
            Some(true),
            Some(true)
        )?,
        "مي روم"
    );
    Ok(())
}

fn doc(items: &[(&str, i64, i64, &str)]) -> Document {
    Document {
        kind: "srt".into(),
        cues: items
            .iter()
            .map(|&(id, start, end, body)| Cue {
                id: id.into(),
                start,
                end,
                text: body.into(),
                ..Cue::default()
            })
            .collect(),
        ..Document::default()
    }
}

#[test]
fn ass_profile_records_watermark_and_floors_timing() -> Check {
    let input = doc(&[
        (
            "one",
            1001,
            2009,
            r##"{an8}<font color="#123456"><i>می روم.</i></font>"##,
        ),
        ("two", 3000, 5000, "Sous-titrage : A"),
    ]);
    let (result, removed) = conversion::convert(
        &input,
        &Options {
            title: "Episode",
            ..Options::default()
        },
    )?;
    let payload = document::serialize(&result, &result.cues, false)?;
    assert!(
        payload
            .contains("PlayResX: 1920\nPlayResY: 1080\nScaledBorderAndShadow: yes\nWrapStyle: 0")
    );
    assert!(payload.contains(document::DEFAULT_STYLE));
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0]["id"], "two");
    assert_eq!(removed[0]["reason"], "studio-credit");
    let cue = &document::parse(&payload, "ass")?.cues[0];
    assert_eq!((cue.start, cue.end), (1000, 2000));
    assert!(cue.text.starts_with(r"{\an8}{\c&H563412&}{\i1}"));
    assert_eq!(visible(cue.text.clone(), "ass".into())?, "می‌روم.");
    assert_eq!((input.cues[0].start, input.cues.len()), (1001, 2));
    Ok(())
}

#[test]
fn translator_slots_are_sorted_with_interleaved_source_comments() -> Check {
    let primary = doc(&[
        ("a", 4100, 5000, "چه خبر؟"),
        ("b", 1000, 2000, "<i>می روم.</i>"),
    ]);
    let donor = doc(&[
        ("a", 1100, 1900, "Covered"),
        ("b", 1800, 2600, "Quarter overlap"),
        ("c", 3000, 3900, "Missing"),
        ("d", 5200, 6000, "Addic7ed"),
    ]);
    let donors = [
        ("Dutch".into(), donor.clone()),
        ("Duplicate".into(), donor.clone()),
    ];
    let (edition, stats) =
        conversion::translator_ass(&primary, &donors, true, &Options::default())?;
    assert_eq!(
        edition.cues.iter().map(|cue| cue.start).collect::<Vec<_>>(),
        [1000, 1800, 3000, 4100]
    );
    assert_eq!(
        edition
            .cues
            .iter()
            .map(|cue| visible(cue.text.clone(), "ass".into()))
            .collect::<Result<Vec<_>, _>>()?,
        ["می‌روم.", "", "", "چه خبر؟"]
    );
    assert_eq!(stats["primary_cues"], 2);
    assert_eq!(stats["placeholder_slots"], 2);
    assert_eq!(stats["watermarks_removed"], 2);
    let payload = document::serialize(&edition, &edition.cues, false)?;
    assert!(payload.contains("Dutch: Quarter overlap") && payload.contains("Dutch: Covered"));
    assert!(
        payload
            .lines()
            .zip(payload.lines().skip(1))
            .any(|(a, b)| a.starts_with("Comment:")
                && a.ends_with("Duplicate: Missing")
                && b.starts_with("Dialogue:")
                && b.contains("0:00:03.00,0:00:03.90,")
                && b.ends_with(','))
    );
    let reparsed = document::parse(&payload, "ass")?;
    assert!(
        reparsed
            .cues
            .iter()
            .map(|c| &c.text)
            .eq(edition.cues.iter().map(|c| &c.text))
    );
    let (without, _) = conversion::translator_ass(
        &primary,
        &[("Dutch".into(), donor)],
        false,
        &Options::default(),
    )?;
    assert!(!document::serialize(&without, &without.cues, false)?.contains("Comment:"));
    Ok(())
}

#[test]
fn bundled_b_koodak_font_exists() -> Check {
    assert!(
        assets::bundled_fonts_dir()
            .ok_or("font directory")?
            .join("BKOODB.TTF")
            .is_file()
    );
    Ok(())
}

#[test]
fn defaults_to_ass_and_explicit_srt_remains_loadable() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path())?;
    let ass = workspace::build(&work, None)?;
    assert_eq!(ass["episodes"][0]["file"], "Sub/S01E01.ass");
    assert_eq!(
        ass["episodes"][0]["provenance"][0]["timing"]["conversion"],
        "floor-centisecond"
    );
    let doc = delivery::load_build(&built_dir(&ass)?)?.1;
    assert_eq!(doc["S01E01"].cues.len(), 1);
    assert_eq!(
        visible(doc["S01E01"].cues[0].text.clone(), "ass".into())?,
        "می‌روم."
    );
    let srt = workspace::build(&work, Some("srt"))?;
    assert_ne!(ass["identity"], srt["identity"]);
    assert_eq!(srt["episodes"][0]["file"], "Sub/S01E01.srt");
    assert_eq!(
        delivery::load_build(&built_dir(&srt)?)?.1["S01E01"].kind,
        "srt"
    );
    assert_eq!(
        workspace::read_record(&work, "project.json")?["output_format"],
        "ass"
    );
    Ok(())
}

macro_rules! physical_newlines {
    ($($name:ident: $nl:literal),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let doc=doc(&[("c000001",1000,2000,"original")]);
            let row=json!({"id":"c000001","source_text":"original","text":format!("سلام{}دنیا",$nl),"reviewed":true,"action":"edit","start_ms":1000,"end_ms":2000});
            let (cues,_)=review::reviewed_cues(&doc,&[row],"fa")?;
            assert!(!cues[0].text.contains('\r'));
            assert_eq!(cues[0].text.matches(RTL).count(),2);
            assert_eq!(document::parse(&document::serialize(&doc,&cues,false)?,"srt")?.cues[0].text,cues[0].text);
            Ok(())
        }
    )+};
}
physical_newlines! { review_lf: "\n", review_cr: "\r", review_crlf: "\r\n" }

#[test]
fn srt_donor_crlf_becomes_one_hard_ass_break() -> Check {
    let original = doc(&[("c000001", 1000, 2000, "<i>Hello</i>\r\nWorld")]);
    let cue = document::srt_to_ass(original.cues[0].clone())?;
    assert!(!cue.text.contains(['\r', '\n']));
    assert_eq!(cue.text, r"{\i1}Hello{\i0}\NWorld");
    Ok(())
}

#[test]
fn timestamp_bounds_and_ascii_digits_match_the_parser() -> Check {
    for (kind, accepted, bad) in [
        (
            "srt",
            "277777:46:40,000",
            [
                "300000:00:00,000",
                "00:00:00,۰۰۱",
                "99999999999999999999:00:00,000",
            ],
        ),
        (
            "ass",
            "277777:46:40.00",
            [
                "300000:00:00.00",
                "0:00:00.۰۱",
                "99999999999999999999:00:00.00",
            ],
        ),
    ] {
        assert_eq!(document::timestamp(accepted, kind)?, 1_000_000_000_000);
        for value in bad {
            assert!(document::timestamp(value, kind).is_err(), "{value}");
        }
    }
    assert!(document::effective_times(1_000_000_000_000, 1_000_000_000_001, "srt").is_err());
    Ok(())
}

#[test]
fn out_of_range_srt_refuses_without_publishing_work() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("large.srt");
    let work = root.path().join("work");
    let body = "1\n300000:00:00,000 --> 300000:00:01,000\nHello.\n";
    write(&input, body)?;
    err(prepare(&input, &work), "timestamp");
    assert!(!work.exists());
    assert_eq!(fs::read(input).map_err(|e| e.to_string())?, body.as_bytes());
    Ok(())
}

#[test]
fn changed_source_and_noncanonical_episode_are_refused() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path())?;
    let old = workspace::read_record(&work, "project.json")?;
    let mut invalid = old.clone();
    invalid["episodes"][0]["id"] = json!("S01E001");
    files::write_json(&work.join("project.json"), &invalid)?;
    err(workspace::build(&work, None), "canonical");
    files::write_json(&work.join("project.json"), &old)?;
    write(&work.join("sources/s0001.srt"), format!("{SRT}changed"))?;
    err(workspace::build(&work, None), "Source changed");
    assert!(!work.join("builds").exists());
    Ok(())
}

#[test]
fn modified_existing_glossary_is_not_silently_replaced() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path())?;
    let path = built_dir(&workspace::build(&work, None)?)?.join("glossary.json");
    let changed = json!({"series":"Not the reviewed glossary"});
    files::write_json(&path, &changed)?;
    err(workspace::build(&work, None), "glossary");
    assert_eq!(files::read_json(&path, 8192)?, changed);
    Ok(())
}

fn archived(method: CompressionMethod) -> Result<Vec<u8>, String> {
    if method == CompressionMethod::Lzma {
        // ZIP method 14 is decoded by zip, but its writer cannot produce it.
        return b"504b03043f0002000e00000021004ccc2adc59000000640000000b000000657069736f64652e737274090405005d00008000001882825c0c662cddc06df6d685aabae45913fcabbbbc374ce8fdf42f7bdad0ca2897c44c1717ca647dcb3027f5012b72f1935083a96796633351dc034ae8a1650256b1439d25b484ab8fff6570c000504b01023f003f0002000e00000021004ccc2adc59000000640000000b0000000000000000000000800100000000657069736f64652e737274504b0506000000000100010039000000820000000000"
            .as_chunks::<2>().0.iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).map_err(|e| e.to_string())?, 16).map_err(|e| e.to_string()))
            .collect();
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "episode.srt",
        SimpleFileOptions::default().compression_method(method),
    )
    .map_err(|e| e.to_string())?;
    zip.write_all(SRT.as_bytes()).map_err(|e| e.to_string())?;
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

macro_rules! zip_methods {
    ($($name:ident: $method:ident),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root=tempfile::tempdir().map_err(|e|e.to_string())?;
            let archive=root.path().join("subtitle.zip");
            let work=root.path().join("work");
            let raw=archived(CompressionMethod::$method)?;
            write(&archive,&raw)?;
            let result=prepare(&archive,&work)?;
            assert_eq!(result["sources"][0]["origin"]["member"],"episode.srt");
            assert_eq!(fs::read(work.join("sources/s0001.srt")).map_err(|e|e.to_string())?,SRT.as_bytes());
            assert_eq!(fs::read(&archive).map_err(|e|e.to_string())?,raw);
            Ok(())
        }
    )+};
}
zip_methods! { stored_zip_imports: Stored, deflate_zip_imports: Deflated, bzip2_zip_imports: Bzip2, lzma_zip_imports: Lzma }

macro_rules! invalid_streams {
    ($($name:ident: ($method:ident, $offset:literal, $byte:literal)),+ $(,)?) => {$ (
        #[test]
        fn $name() -> Check {
            let root = tempfile::tempdir().map_err(|e| e.to_string())?;
            let mut bytes = archived(CompressionMethod::$method)?;
            let data = 30
                + usize::from(u16::from_le_bytes([bytes[26], bytes[27]]))
                + usize::from(u16::from_le_bytes([bytes[28], bytes[29]]));
            bytes[data + $offset] = $byte;
            let archive = root.path().join("corrupt.zip");
            let work = root.path().join("work");
            write(&archive, &bytes)?;
            let error = prepare(&archive,&work).expect_err("corrupt compressed stream");
            assert!(error.contains("compressed stream") && !error.contains("Traceback"),"{error}");
            assert!(!work.exists());
            assert_eq!(fs::read(archive).map_err(|e| e.to_string())?, bytes);
            Ok(())
        }
    )+};
}
invalid_streams! {
    corrupt_deflate_member_leaves_no_workspace: (Deflated, 0, 7),
    corrupt_bzip2_member_leaves_no_workspace: (Bzip2, 0, 0),
    corrupt_lzma_member_leaves_no_workspace: (Lzma, 4, 255),
}

#[test]
fn unsupported_zip_compression_is_reported_before_publication() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("unsupported.zip");
    let work = root.path().join("work");
    let mut bytes = archived(CompressionMethod::Stored)?;
    let central = bytes
        .windows(4)
        .position(|s| s == b"PK\x01\x02")
        .ok_or("central directory")?;
    bytes[8..10].copy_from_slice(&98_u16.to_le_bytes());
    bytes[central + 10..central + 12].copy_from_slice(&98_u16.to_le_bytes());
    write(&path, &bytes)?;
    err(prepare(&path, &work), "Unsupported ZIP compression");
    assert!(!work.exists());
    Ok(())
}

#[test]
fn no_clobber_publication_is_retryable_after_destination_is_free() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("archive.zip");
    write(&path, b"owned by caller")?;
    assert!(publish_bytes(&path, b"complete").is_err());
    assert_eq!(
        fs::read(&path).map_err(|e| e.to_string())?,
        b"owned by caller"
    );
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    publish_bytes(&path, b"complete").map_err(|e| e.to_string())?;
    assert_eq!(fs::read(&path).map_err(|e| e.to_string())?, b"complete");
    Ok(())
}

#[test]
fn animation_is_sampled_at_each_repeated_event() -> Check {
    let mut track = doc(&[
        ("a", 1000, 2000, r"{\move(0,0,20,20)}Test"),
        ("b", 3000, 4000, r"{\move(0,0,20,20)}Test"),
        ("c", 5000, 6000, r"{\move(0,0,20,20)}Test"),
    ]);
    track.kind = "ass".into();
    let samples = sampling::samples(&track, false, &[])?;
    for (offset, times) in [[1.1, 1.5, 1.9], [3.1, 3.5, 3.9], [5.1, 5.5, 5.9]]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            samples
                .iter()
                .filter(|s| s["cue_indices"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(offset + 1))))
                .map(|s| &s["time_seconds"])
                .collect::<Vec<_>>(),
            times
        );
    }
    assert_eq!(sampling::SAMPLER_VERSION, 12);
    Ok(())
}

#[test]
fn out_of_range_ass_refuses_without_publishing_work() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("large.ass");
    let work = root.path().join("work");
    let body = format!(
        "[Script Info]\nScriptType: v4.00+\n[V4+ Styles]\nFormat: {}\nStyle: {}\n[Events]\nFormat: {}\nDialogue: 0,300000:00:00.00,300000:00:01.00,Default,,0,0,0,,Hello.\n",
        document::STYLE_FIELDS,
        document::DEFAULT_STYLE,
        document::ASS_FIELDS
    );
    write(&input, &body)?;
    err(prepare(&input, &work), "timestamp");
    assert!(!work.exists());
    assert_eq!(fs::read(input).map_err(|e| e.to_string())?, body.as_bytes());
    Ok(())
}

#[test]
fn sampled_animation_variants_cover_each_event() -> Check {
    for tag in [
        r"\move(0,0,10,10)",
        r"\fad(100,100)",
        r"\fade(255,0,255,0,100,800,1000)",
        r"\t(0,100,\fs50)",
        r"\k10",
        r"\K10",
        r"\kf10",
        r"\ko10",
        r"\kt10",
    ] {
        let body = format!("{{{tag}}}متن");
        let mut track = doc(&[
            ("a", 1000, 2000, &body),
            ("b", 3000, 4000, &body),
            ("c", 5000, 6000, &body),
        ]);
        track.kind = "ass".into();
        let plan = sampling::samples(&track, false, &[])?;
        let selected: std::collections::BTreeSet<_> = plan
            .iter()
            .flat_map(|s| s["cue_indices"].as_array().into_iter().flatten())
            .filter_map(Value::as_i64)
            .collect();
        assert_eq!(selected, [1, 2, 3].into_iter().collect(), "{tag}");
        let middle = plan
            .iter()
            .filter(|s| {
                s["cue_indices"]
                    .as_array()
                    .is_some_and(|indices| indices.contains(&json!(2)))
            })
            .map(|s| &s["time_seconds"])
            .collect::<Vec<_>>();
        assert_eq!(middle, [3.1, 3.5, 3.9], "{tag}");
    }
    Ok(())
}
