use anyhow::{Result, anyhow};
use revayat_core::subtitle_formats::document::{ASS_FIELDS, Document, parse};
use std::{fs, process::Command};

fn source(layout: &str, styles: &str, style: &str, text: &str) -> String {
    format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\
         [V4+ Styles]\nFormat: {layout}\n{styles}\n\
         [Events]\nFormat: {ASS_FIELDS}\nDialogue: 0,0:00:00.00,0:00:01.00,{style},,0,0,0,,{text}\n"
    )
}

fn convert(subcommand: &str, input: &str, flags: &[&str]) -> Result<Document> {
    let scratch = tempfile::tempdir()?;
    fs::write(scratch.path().join("input.ass"), input)?;
    let output = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"))
        .current_dir(scratch.path())
        .env("REVAYAT_LOG_DIR", scratch.path())
        .args([
            subcommand,
            "input.ass",
            "--out",
            "output.ass",
            "--direction",
            "ltr",
        ])
        .args(flags)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse(
        &fs::read_to_string(scratch.path().join("output.ass"))?,
        "ass",
    )
    .map_err(|e| anyhow!(e))
}

fn cell<'a>(doc: &'a Document, name: &str, field: &str) -> &'a str {
    let index = doc
        .style_fields
        .iter()
        .position(|key| key == field)
        .expect(field);
    &doc.styles[name][index]
}

#[test]
fn bold_comes_from_the_exact_default_declaration() -> Result<()> {
    for command in ["convert", "translator-ass"] {
        for (bold, other) in [("-1", "0"), ("0", "-1")] {
            let styles = format!("Style: default,{other}\nStyle: **Default,{bold}");
            let input = source(
                "Name,Bold",
                &styles,
                "dEfAuLt",
                r"Dialogue {\rdefault}reset",
            );
            let output = convert(command, &input, &[])?;
            assert_eq!(cell(&output, "Default", "bold"), bold);
            assert_eq!(cell(&output, "default", "bold"), other);
            assert!(output.cues[0].text.contains(r"{\rdefault}"));
        }
    }
    Ok(())
}

#[test]
fn abbreviated_layout_keeps_generated_options_and_other_styles() -> Result<()> {
    let input = source(
        "Fontsize,Name,Outline,Custom",
        "Style: 80,Default,4,default label\nStyle: 34,Sign,1.25,author label",
        "Default",
        r"Dialogue {\rSign}sign",
    );
    for command in ["convert", "translator-ass"] {
        let output = convert(
            command,
            &input,
            &["--bold", "--margin-l", "27", "--margin-v", "5"],
        )?;
        for (field, expected) in [
            ("fontsize", "110"),
            ("fontname", "B Koodak"),
            ("bold", "-1"),
            ("outline", "2"),
            ("shadow", "2"),
            ("marginl", "27"),
            ("marginr", "10"),
            ("marginv", "5"),
            ("custom", "default label"),
        ] {
            assert_eq!(cell(&output, "Default", field), expected, "{field}");
        }
        for (field, expected) in [
            ("fontsize", "34"),
            ("fontname", "Arial"),
            ("bold", "0"),
            ("outline", "1.25"),
            ("shadow", "0"),
            ("scalex", "100"),
            ("scaley", "100"),
            ("marginl", "0"),
            ("custom", "author label"),
        ] {
            assert_eq!(cell(&output, "Sign", field), expected, "{field}");
        }
        assert!(output.cues[0].text.contains(r"{\rSign}"));
    }
    Ok(())
}

#[test]
fn numeric_source_bold_obeys_renderer_integer_semantics_and_explicit_override() -> Result<()> {
    for (source_bold, inherited) in [
        ("0.5", "0"),
        ("-0.5", "0"),
        ("4294967296", "0"),
        ("4294967297", "-1"),
        ("&HFFFFFFFF", "-1"),
        ("0x1", "-1"),
        ("&H0x1", "-1"),
        ("&H-0x1", "-1"),
        ("0x0XFF", "-1"),
        ("\t &H +0xFF", "-1"),
        ("\u{a0}&H1", "0"),
        ("&H-0x100000000", "0"),
        ("&h0", "0"),
        ("&H1&", "-1"),
        ("0X100000000", "0"),
    ] {
        let input = source(
            "Name,Bold",
            &format!("Style: Default,{source_bold}"),
            "Default",
            "Text",
        );
        let output = convert("convert", &input, &[])?;
        assert_eq!(cell(&output, "Default", "bold"), inherited);
        let output = convert("translator-ass", &input, &["--no-bold"])?;
        assert_eq!(cell(&output, "Default", "bold"), "0");
    }
    Ok(())
}

#[test]
fn widening_a_custom_format_keeps_implicit_border_scaling() -> Result<()> {
    use revayat_core::subtitle_formats::document::{DEFAULT_STYLE, STYLE_FIELDS};

    let layout = STYLE_FIELDS.strip_suffix(", Encoding").unwrap();
    let style = DEFAULT_STYLE.strip_suffix(",1").unwrap();
    let input = source(layout, &format!("Style: {style}"), "Default", "Dialogue");
    let output = convert("convert", &input, &[])?;
    assert_eq!(cell(&output, "Default", "encoding"), "1");
    // With no explicit ScaledBorderAndShadow, libass enables it for a custom format.
    let canonical = STYLE_FIELDS
        .split(',')
        .map(|key| key.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    assert_ne!(output.style_fields, canonical);
    Ok(())
}

#[test]
fn reviewed_workspace_adds_complete_srt_style_then_merges_a_compatible_ass_donor() -> Result<()> {
    use revayat_core::runtime::files;
    use serde_json::{Value, json};

    let scratch = tempfile::tempdir()?;
    let root = scratch.path();
    let work = root.join("work");
    let layout = "Name,Fontsize,Custom";
    let native = |name: &str| source(layout, &format!("Style: {name},43,credit"), name, name);
    fs::write(root.join("signs.ass"), native("Signs"))?;
    fs::write(root.join("lyrics.ass"), native("Lyrics"))?;
    fs::write(
        root.join("dialogue.srt"),
        "1\n00:00:02,000 --> 00:00:03,000\nDialogue\n",
    )?;
    let run = |args: &[&str]| -> Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"))
            .current_dir(root)
            .env("REVAYAT_LOG_DIR", root)
            .args(args)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    run(&[
        "prepare",
        "signs.ass",
        "dialogue.srt",
        "lyrics.ass",
        "--work",
        "work",
        "--series",
        "Authored",
        "--season",
        "1",
        "--target-language",
        "en",
    ])?;
    let project_path = work.join("project.json");
    let mut project: Value = serde_json::from_slice(&fs::read(&project_path)?)?;
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","alternates":["s0002","s0003"],"comparison":"Dialogue follows the sign, and lyrics come from the final source."}]);
    fs::write(project_path, serde_json::to_vec(&project)?)?;
    for id in ["s0001", "s0002", "s0003"] {
        let file = work.join(format!("worksheets/{id}.json"));
        let mut rows: Value = serde_json::from_slice(&fs::read(&file)?)?;
        rows[0]["reviewed"] = json!(true);
        rows[0]["action"] = json!("preserve");
        fs::write(file, serde_json::to_vec(&rows)?)?;
    }
    let glossary = json!({"series":"Authored","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Authored mechanical fixture"}]});
    files::write_json(&work.join("glossary.json"), &glossary).map_err(|e| anyhow!(e))?;
    let manifest = run(&["build", "--work", "work"])?;
    let file = std::path::Path::new(manifest["build"].as_str().unwrap()).join("Sub/S01E01.ass");
    let edition = parse(&fs::read_to_string(file)?, "ass").map_err(|e| anyhow!(e))?;
    for (field, value) in [
        ("fontname", "B Koodak"),
        ("fontsize", "110"),
        ("bold", "0"),
        ("outline", "2"),
        ("shadow", "2"),
        ("marginl", "10"),
        ("marginr", "10"),
        ("marginv", "10"),
    ] {
        assert_eq!(cell(&edition, "Default", field), value);
    }
    for style in ["Signs", "s0003__Lyrics"] {
        assert_eq!(cell(&edition, style, "fontsize"), "43");
        assert_eq!(cell(&edition, style, "fontname"), "Arial");
        assert_eq!(cell(&edition, style, "marginl"), "0");
        assert_eq!(cell(&edition, style, "custom"), "credit");
    }
    assert_eq!(edition.cues.len(), 3);
    assert_eq!(manifest["generation_recipe"]["version"], 17);
    Ok(())
}

#[test]
fn widening_never_masks_an_incompatible_source_layout() -> Result<()> {
    use revayat_core::workflow::review::donor;

    let sources = [
        source(
            "Fontsize,Custom,Name",
            "Style: 40,base note,Signs",
            "Signs",
            "Base",
        ),
        source("Custom,Name", "Style: donor note,Lyrics", "Lyrics", "Donor"),
    ];
    let mut base = parse(&sources[0], "ass").map_err(|e| anyhow!(e))?;
    let srt =
        parse("1\n00:00:02,000 --> 00:00:03,000\nDialogue\n", "srt").map_err(|e| anyhow!(e))?;
    donor(&mut base, &srt, srt.cues.clone(), "s0002", true).map_err(|e| anyhow!(e))?;
    let ass = parse(&sources[1], "ass").map_err(|e| anyhow!(e))?;
    let before = base.clone();
    let error = donor(&mut base, &ass, ass.cues.clone(), "s0003", true).unwrap_err();
    assert!(error.contains("formats differ"), "{error}");
    assert_eq!(base, before);
    Ok(())
}
