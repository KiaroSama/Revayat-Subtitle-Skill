use super::{
    contract::io_error,
    import::{ImportOptions, prepare},
    workspace::{build, read_record},
};
use crate::runtime::files::write_json;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn fixture() -> Result<(tempfile::TempDir, PathBuf), String> {
    let temp = tempfile::tempdir().map_err(io_error)?;
    let work = temp.path().join("review work");
    let inputs = [temp.path().join("Base.srt"), temp.path().join("Donor.srt")];
    fs::write(
        &inputs[0],
        "1\n00:00:01,023 --> 00:00:03,049\nRetained dialogue\n\n2\n00:00:04,000 --> 00:00:06,000\nSubtitles by Alice\n",
    )
    .map_err(io_error)?;
    fs::write(
        &inputs[1],
        "1\n00:00:04,000 --> 00:00:06,000\nAlternate source text\n",
    )
    .map_err(io_error)?;
    prepare(
        &inputs,
        &work,
        &ImportOptions {
            series: "Review fixture",
            season: 1,
            encoding: "utf-8",
            glossary: None,
            language: "en",
            output_format: "ass",
        },
    )?;
    let mut project = read_record(&work, "project.json")?;
    project["episodes"] = json!([{"id":"S01E01","base":"s0001","alternates":["s0002"],"comparison":"Preserve the first candidate"}]);
    write_json(&work.join("project.json"), &project)?;
    write_json(
        &work.join("glossary.json"),
        &json!({"series":"Review fixture","terms_reviewed":true,"terms":[],"research":[{"url":"https://example.org/fixture","note":"Fixture evidence"}]}),
    )?;
    for key in ["s0001", "s0002"] {
        let name = format!("worksheets/{key}.json");
        let mut rows = read_record(&work, &name)?;
        for row in rows.as_array_mut().into_iter().flatten() {
            row["action"] = json!("preserve");
            row["reviewed"] = json!(true);
        }
        write_json(&work.join(name), &rows)?;
    }
    Ok((temp, work))
}

#[test]
fn final_emission_rejects_self_links_and_links_to_removed_watermarks() -> Result<(), String> {
    let (_temp, work) = fixture()?;
    let sheet = "worksheets/s0002.json";
    let mut rows = read_record(&work, sheet)?;
    rows[0]["action"] = json!("alternate");
    rows[0]["note"] = json!("Already covered by another source.");
    rows[0]["links"] = json!(["s0002:c000001"]);
    write_json(&work.join(sheet), &rows)?;
    assert_eq!(
        build(&work, None).err().as_deref(),
        Some("ValueError: A review link cannot target itself")
    );
    rows[0]["links"] = json!(["s0001:c000002"]);
    write_json(&work.join(sheet), &rows)?;
    assert_eq!(
        build(&work, None).err().as_deref(),
        Some(
            "ValueError: Review links must identify emitted cues; a linked target was removed during normalization"
        )
    );
    assert!(!work.join("builds").exists());
    rows[0]["links"] = json!(["s0001:c000001"]);
    write_json(&work.join(sheet), &rows)?;
    let edition = build(&work, None)?;
    assert_eq!(
        edition["generation_recipe"],
        json!({"version":16,"normalization":13,"timing":"floor-centisecond"})
    );
    assert_eq!(
        edition["episodes"][0]["provenance"][0]["timing"]["emitted"],
        json!([1020, 3040])
    );
    assert_eq!(edition["episodes"][0]["cues"], 1);
    assert_eq!(edition, build(&work, None)?);
    Ok(())
}
