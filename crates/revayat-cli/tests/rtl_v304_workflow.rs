//! Public CLI regressions for ASS escape-aware bidi level checks.

use anyhow::{Result, anyhow};
use revayat_core::{runtime::files, subtitle_formats::document};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

const RLM: &str = "\u{200f}";
const LRM: &str = "\u{200e}";
const RLI: &str = "\u{2067}";
const FSI: &str = "\u{2068}";
const PDI: &str = "\u{2069}";
const LRE: &str = "\u{202a}";
const PDF: &str = "\u{202c}";

fn command(root: &Path, arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"))
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null())
        .args(arguments)
        .output()?)
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn boundary(body: &str, direction: &str) -> String {
    if direction == "rtl" {
        format!(
            "NASA سلام {}{RLI}{LRE}{FSI}{body}{PDI}{PDF}{PDI}{} پایان",
            LRE.repeat(60),
            PDF.repeat(60)
        )
    } else {
        format!(
            "NASA سلام {}{FSI}{body}{PDI}{} پایان",
            LRE.repeat(61),
            PDF.repeat(61)
        )
    }
}

fn isolated(text: &str) -> String {
    format!("{RLM}{RLI}{text}{PDI}")
}

fn ass(texts: &[String], wrap_style: i32, effect: &str) -> String {
    let mut output = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\nWrapStyle: {wrap_style}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,DejaVu Sans,70,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,30,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    for (i, text) in texts.iter().enumerate() {
        output.push_str(&format!(
            "Dialogue: 0,0:00:{:02}.00,0:00:{:02}.00,Default,,0,0,0,{effect},{text}\n",
            i * 3,
            i * 3 + 2
        ));
    }
    output
}

fn convert_texts(source: &str) -> Result<Vec<String>> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::write(root.join("input.ass"), source)?;
    success(&command(
        root,
        &[
            "convert",
            "input.ass",
            "--out",
            "output.ass",
            "--max-chars",
            "4096",
            "--keep-watermarks",
            "--no-zwnj",
        ],
    )?);
    let first = document::parse(&fs::read_to_string(root.join("output.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    success(&command(
        root,
        &[
            "convert",
            "output.ass",
            "--out",
            "second.ass",
            "--max-chars",
            "4096",
            "--keep-watermarks",
            "--no-zwnj",
        ],
    )?);
    let second = document::parse(&fs::read_to_string(root.join("second.ass"))?, "ass")
        .map_err(anyhow::Error::msg)?;
    let texts: Vec<_> = first.cues.into_iter().map(|cue| cue.text).collect();
    assert_eq!(
        second
            .cues
            .into_iter()
            .map(|cue| cue.text)
            .collect::<Vec<_>>(),
        texts
    );
    assert_eq!(fs::read_to_string(root.join("input.ass"))?, source);
    Ok(texts)
}

#[test]
fn convert_uses_visible_fsi_direction_for_ass_spaces() -> Result<()> {
    let texts: Vec<_> = [r"\hدنیا!", "\u{00a0}دنیا!", r"\nدنیا!", " دنیا!"]
        .into_iter()
        .map(|body| boundary(body, "rtl"))
        .collect();
    let expected: Vec<_> = texts.iter().map(|text| isolated(text)).collect();
    assert_eq!(convert_texts(&ass(&texts, 0, ""))?, expected);
    Ok(())
}

#[test]
fn convert_distinguishes_wrap_style_two_from_inline_soft_space_overrides() -> Result<()> {
    let source = boundary(r"\nدنیا!", "rtl");
    let texts = vec![
        source.clone(),
        format!(r"{{\q0}}{source}"),
        format!(r"{{\q0}}{{\q}}{source}"),
        format!(r"{{\q0}}{{\rDefault}}{source}"),
    ];
    let expected = vec![
        source.clone(),
        format!(r"{{\q0}}{}", isolated(&source)),
        format!(r"{{\q0}}{{\q}}{source}"),
        format!(r"{{\q0}}{{\rDefault}}{}", isolated(&source)),
    ];
    assert_eq!(convert_texts(&ass(&texts, 2, ""))?, expected);
    Ok(())
}

#[test]
fn banner_soft_breaks_remain_breaks_until_an_inline_override() -> Result<()> {
    let source = boundary(r"\nدنیا!", "rtl");
    let texts = vec![source.clone(), format!(r"{{\q0}}{source}")];
    let expected = vec![source.clone(), format!(r"{{\q0}}{}", isolated(&source))];
    for effect in ["Banner;20;0", " \tBanner;20;0"] {
        assert_eq!(convert_texts(&ass(&texts, 0, effect))?, expected);
    }
    Ok(())
}

fn reviewed_texts(
    texts: &[String],
    wrap_style: i32,
    effect: &str,
    directions: &[&str],
) -> Result<Vec<String>> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let source = ass(texts, wrap_style, effect);
    fs::write(root.join("S01E01.ass"), &source)?;
    success(&command(
        root,
        &[
            "prepare",
            "S01E01.ass",
            "--work",
            "work",
            "--series",
            "Authored FSI Audit",
            "--season",
            "1",
            "--output-format",
            "ass",
        ],
    )?);
    let work = root.join("work");
    let project_path = work.join("project.json");
    let mut project: Value = serde_json::from_slice(&fs::read(&project_path)?)?;
    project["episodes"] = json!([{"id": "S01E01", "base": "s0001", "alternates": [], "comparison": "Authored FSI escape fixture"}]);
    files::write_json(&project_path, &project).map_err(|error| anyhow!(error))?;
    files::write_json(&work.join("glossary.json"), &json!({
        "series": "Authored FSI Audit", "terms_reviewed": true, "terms": [],
        "research": [{"url": "https://www.unicode.org/reports/tr9/", "note": "Authored technical fixture for FSI visible text direction"}]
    })).map_err(|error| anyhow!(error))?;
    let sheet_path = work.join("worksheets/s0001.json");
    let mut sheet: Value = serde_json::from_slice(&fs::read(&sheet_path)?)?;
    for (row, direction) in sheet
        .as_array_mut()
        .expect("worksheet rows")
        .iter_mut()
        .zip(directions)
    {
        row["reviewed"] = json!(true);
        row["action"] = json!("preserve");
        row["direction"] = json!(direction);
        row["direction_note"] = json!("Explicit authored test direction");
    }
    files::write_json(&sheet_path, &sheet).map_err(|error| anyhow!(error))?;
    let built = command(root, &["build", "--work", "work"])?;
    success(&built);
    let manifest: Value = serde_json::from_slice(&built.stdout)?;
    let emitted = Path::new(manifest["build"].as_str().expect("build path")).join(
        manifest["episodes"][0]["file"]
            .as_str()
            .expect("episode file"),
    );
    let output =
        document::parse(&fs::read_to_string(emitted)?, "ass").map_err(anyhow::Error::msg)?;
    assert_eq!(fs::read_to_string(root.join("S01E01.ass"))?, source);
    Ok(output.cues.into_iter().map(|cue| cue.text).collect())
}

#[test]
fn reviewed_native_ass_build_uses_escape_aware_rtl_and_ltr_wrappers() -> Result<()> {
    let texts = vec![boundary(r"\hدنیا!", "rtl"), boundary(r"\hدنیا!", "ltr")];
    let output = reviewed_texts(&texts, 0, "", &["rtl", "ltr"])?;
    assert_eq!(output[0], isolated(&texts[0]));
    assert_eq!(output[1], format!("{LRE}{LRM}{}{LRM}{PDF}", texts[1]));
    Ok(())
}

#[test]
fn reviewed_soft_spaces_follow_header_inline_and_banner_context() -> Result<()> {
    let outer = |text: &str, direction: &str| {
        if direction == "rtl" {
            isolated(text)
        } else {
            format!("{LRE}{LRM}{text}{LRM}{PDF}")
        }
    };
    for (name, header, tags, effect, soft_space) in [
        ("header0", 0, "", "", true),
        ("inline_q2", 0, r"{\q2}", "", false),
        ("header2", 2, "", "", false),
        ("header2_q0", 2, r"{\q0}", "", true),
        ("empty_q", 2, r"{\q0}{\q}", "", false),
        ("invalid_q", 2, r"{\q0}{\q9}", "", false),
        ("style_reset", 2, r"{\q0}{\rDefault}", "", true),
        ("banner", 0, "", "Banner;20;0", false),
        ("banner_q0", 0, r"{\q0}", "Banner;20;0", true),
        ("prior_line", 2, r"{\q2}{\q0}", "", true),
    ] {
        let mut texts = Vec::new();
        let mut expected = Vec::new();
        let mut directions = Vec::new();
        for direction in ["rtl", "ltr"] {
            for space in [r"\n", " "] {
                let source = boundary(&format!("{space}دنیا!"), direction);
                let result = if space == " " || soft_space {
                    outer(&source, direction)
                } else {
                    let (first, last) = source.split_once(r"\n").expect("authored soft break");
                    let mark = if direction == "rtl" { RLM } else { LRM };
                    format!(r"{mark}{first}{mark}\n{mark}{last}{mark}")
                };
                if name == "prior_line" {
                    texts.push(format!(r"{tags}سلام A\N{source}"));
                    expected.push(format!(r"{tags}{}\N{result}", outer("سلام A", direction)));
                } else {
                    texts.push(format!("{tags}{source}"));
                    expected.push(format!("{tags}{result}"));
                }
                directions.push(direction);
            }
        }
        assert_eq!(
            reviewed_texts(&texts, header, effect, &directions)?,
            expected,
            "wrong reviewed {name} soft-break context"
        );
    }
    Ok(())
}
