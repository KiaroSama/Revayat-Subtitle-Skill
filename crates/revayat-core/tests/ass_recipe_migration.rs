//! Preserve genuine recipe-19 output when visible ASS direction semantics change.

use revayat_core::{
    render::delivery,
    runtime::files,
    workflow::{
        contract_json,
        import::{self, ImportOptions},
        workspace,
    },
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

type Check = Result<(), String>;

const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\nWrapStyle: 0\n\n[V4+ Styles]\nFormat: name, fontname, fontsize, primarycolour, secondarycolour, outlinecolour, backcolour, bold, italic, underline, strikeout, scalex, scaley, spacing, angle, borderstyle, outline, shadow, alignment, marginl, marginr, marginv, encoding\nStyle: Default,DejaVu Sans,70,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,30,1\n\n[Events]\nFormat: layer, start, end, style, name, marginl, marginr, marginv, effect, text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,";

fn ass(text: &str) -> String {
    format!("{HEADER}{text}\n")
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Check {
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| error.to_string())
}

fn reviewed(root: &Path, text: &str) -> Result<PathBuf, String> {
    let input = root.join("S01E01.ass");
    let work = root.join("work");
    write(&input, ass(text))?;
    import::prepare(
        &[input],
        &work,
        &ImportOptions {
            series: "Authored recipe migration",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "fa",
            output_format: "ass",
        },
    )?;
    let mut project = workspace::read_record(&work, "project.json")?;
    project["episodes"] = json!([{
        "id": "S01E01", "base": "s0001", "alternates": [],
        "comparison": "One authored greeting; no alternate releases"
    }]);
    files::write_json(&work.join("project.json"), &project)?;
    let sheet_path = work.join("worksheets/s0001.json");
    let mut rows = workspace::read_record(&work, "worksheets/s0001.json")?;
    rows[0]["action"] = json!("preserve");
    rows[0]["reviewed"] = json!(true);
    rows[0]["direction"] = json!("ltr");
    rows[0]["direction_note"] = json!("Authored explicit LTR paragraph fixture");
    files::write_json(&sheet_path, &rows)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({
            "series": "Authored recipe migration", "terms": [], "terms_reviewed": true,
            "research": [{"url": "https://www.unicode.org/reports/tr9/", "note": "Authored direction fixture with no franchise terms"}]
        }),
    )?;
    Ok(work)
}

fn check_migration(text: &str, had_false_latin_character: bool) -> Check {
    let root = tempfile::tempdir().map_err(|error| error.to_string())?;
    let work = reviewed(root.path(), text)?;
    let edition = workspace::assemble(&work, None)?;
    let current_payload = ass(&format!("\u{200e}{text}\u{200e}"));
    assert_eq!(edition.files["S01E01.ass"], current_payload.as_bytes());

    // These exact wrappers were emitted by fbebb82 (recipe 19/16). Keep the
    // historical bytes independent of the current normalizer under test.
    let historical_text = if had_false_latin_character {
        format!("\u{202a}\u{200e}{text}\u{200e}\u{202c}")
    } else {
        format!("\u{200e}{text}\u{200e}")
    };
    let historical_payload = ass(&historical_text);
    assert_eq!(
        historical_payload != current_payload,
        had_false_latin_character
    );
    let historical_recipe =
        json!({"version": 19, "normalization": 16, "timing": "floor-centisecond"});
    let old_id = workspace::digest(
        contract_json::dump(
            &json!([edition.manifest["input_identity"], historical_recipe, "ass"]),
            true,
            true,
        )?
        .as_bytes(),
    );
    let old_dir = work.join("builds").join(&old_id);
    fs::create_dir_all(old_dir.join("Sub")).map_err(|error| error.to_string())?;
    fs::create_dir(old_dir.join("renders")).map_err(|error| error.to_string())?;
    let mut old = edition.manifest.clone();
    old["identity"] = json!(old_id);
    old["generation_recipe"] = historical_recipe;
    old["episodes"][0]["sha256"] = json!(workspace::digest(historical_payload.as_bytes()));
    files::write_json(&old_dir.join("manifest.json"), &old)?;
    files::write_json(&old_dir.join("glossary.json"), &edition.glossary)?;
    write(&old_dir.join("Sub/S01E01.ass"), &historical_payload)?;

    // This deliberately pending marker tests stale association and preservation;
    // it is not fabricated visual approval or a passing historical QA receipt.
    files::write_json(
        &old_dir.join("renders/S01E01.json"),
        &json!({
            "version": 2, "identity": old_id, "episode": "S01E01",
            "subtitle_sha256": old["episodes"][0]["sha256"], "background": "neutral",
            "all_cues": true, "receipt_file": "renders/S01E01-historical/receipt.json",
            "receipt_sha256": "0".repeat(64), "frames": []
        }),
    )?;
    let protected = [
        root.path().join("S01E01.ass"),
        work.join("project.json"),
        work.join("glossary.json"),
        work.join("sources/s0001.ass"),
        work.join("worksheets/s0001.json"),
        old_dir.join("manifest.json"),
        old_dir.join("glossary.json"),
        old_dir.join("Sub/S01E01.ass"),
        old_dir.join("renders/S01E01.json"),
    ];
    let before: Vec<_> = protected
        .iter()
        .map(|path| read(path))
        .collect::<Result<_, _>>()?;

    // On the regression baseline, changed output collides with the old edition
    // here and falsely reports that the unchanged build was modified.
    let fresh = workspace::build(&work, None)?;
    assert_ne!(fresh["identity"], old["identity"]);
    assert_eq!(fresh["input_identity"], old["input_identity"]);
    assert_eq!(fresh["version"], 2);
    assert!(fresh["generation_recipe"]["version"].as_u64().unwrap() > 19);
    assert!(
        fresh["generation_recipe"]["normalization"]
            .as_u64()
            .unwrap()
            > 16
    );
    assert_eq!(fresh["generation_recipe"]["timing"], "floor-centisecond");
    assert_eq!(fresh, workspace::build(&work, None)?);
    let fresh_dir = Path::new(fresh["build"].as_str().ok_or("missing build path")?);
    assert_eq!(
        read(&fresh_dir.join("Sub/S01E01.ass"))?,
        current_payload.as_bytes()
    );
    assert!(!fresh_dir.join("renders").exists());
    delivery::load_build(fresh_dir)?;
    assert!(delivery::load_build(&old_dir).is_err());
    assert!(delivery::qa(fresh_dir).is_err());
    let archive = root.path().join("unreviewed.zip");
    assert!(delivery::package(fresh_dir, &archive).is_err());
    assert!(!archive.exists());

    fs::create_dir(fresh_dir.join("renders")).map_err(|error| error.to_string())?;
    let stale = fresh_dir.join("renders/S01E01.json");
    fs::copy(old_dir.join("renders/S01E01.json"), &stale).map_err(|error| error.to_string())?;
    assert_eq!(
        delivery::qa(fresh_dir).unwrap_err(),
        "ValueError: Render evidence is stale"
    );
    assert_eq!(
        delivery::package(fresh_dir, &archive).unwrap_err(),
        "ValueError: Render evidence is stale"
    );
    assert!(!archive.exists());
    fs::remove_file(stale).map_err(|error| error.to_string())?;
    let after: Vec<_> = protected
        .iter()
        .map(|path| read(path))
        .collect::<Result<_, _>>()?;
    assert_eq!(after, before);
    Ok(())
}

#[test]
fn hard_space_output_migrates_without_rewriting_recipe_19_editions() -> Check {
    check_migration(r"سلام\hدنیا", true)
}

#[test]
fn soft_space_output_migrates_without_rewriting_recipe_19_editions() -> Check {
    check_migration(r"سلام\nدنیا", true)
}

#[test]
fn unchanged_output_still_uses_the_current_recipe_without_inheriting_approval() -> Check {
    check_migration("سلام دنیا", false)
}
