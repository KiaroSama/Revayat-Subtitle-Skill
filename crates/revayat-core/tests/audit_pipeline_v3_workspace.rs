//! Authored import, identity, and editorial-boundary regressions.

use revayat_core::{
    runtime::files,
    status, validation,
    workflow::{
        contract_json,
        import::{self, ImportOptions},
        workspace,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

type Check = Result<(), String>;
const SRT: &str = "1\n00:00:00,000 --> 00:00:01,000\nAuthored subtitle\n";

fn prepare(input: &Path, work: &Path, encoding: &str) -> Result<Value, String> {
    import::prepare(
        &[input.to_owned()],
        work,
        &ImportOptions {
            series: "Authored",
            season: 1,
            encoding,
            glossary: None,
            language: "en",
            output_format: "srt",
        },
    )
}

fn reviewed(root: &Path, text: &str) -> Result<PathBuf, String> {
    let input = root.join("input.srt");
    let work = root.join("work");
    fs::write(&input, SRT.replace("Authored subtitle", text)).map_err(|e| e.to_string())?;
    prepare(&input, &work, "utf-8")?;
    let mut project = workspace::read_record(&work, "project.json")?;
    project["episodes"] =
        json!([{"id": "S01E01", "base": "s0001", "comparison": "Authored fixture"}]);
    files::write_json(&work.join("project.json"), &project)?;
    let mut sheet = workspace::read_record(&work, "worksheets/s0001.json")?;
    for row in sheet.as_array_mut().ok_or("worksheet array")? {
        row["reviewed"] = json!(true);
        row["action"] = json!("preserve");
    }
    files::write_json(&work.join("worksheets/s0001.json"), &sheet)?;
    files::write_json(
        &work.join("glossary.json"),
        &json!({
            "series": "Authored", "terms": [], "terms_reviewed": true,
            "research": [{"url": "https://example.test", "note": "Authored fixture"}]
        }),
    )?;
    Ok(work)
}

#[test]
fn build_refuses_comment_activation_before_creating_an_edition() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), "A<!<!--note-->--hidden-->B")?;
    let source = fs::read(work.join("sources/s0001.srt")).map_err(|e| e.to_string())?;
    let error = workspace::build(&work, None).expect_err("comment activation must be refused");
    assert!(error.contains("activate literal markup"), "{error}");
    assert!(!work.join("builds").exists());
    assert_eq!(
        fs::read(work.join("sources/s0001.srt")).map_err(|e| e.to_string())?,
        source
    );
    Ok(())
}

#[test]
fn literal_json_markers_cannot_reuse_another_input_identity() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), "Authored subtitle")?;
    let mut project = workspace::read_record(&work, "project.json")?;
    for (marker, scalar) in [
        (json!({"$tuple": [1]}), json!([1])),
        (json!({"$int": "1"}), json!(1)),
        (json!({"$float": "1.0"}), json!(1.0)),
        (json!({"$dict": [["a", 1]]}), json!({"a": 1})),
    ] {
        project["user_metadata"] = marker;
        files::write_json(&work.join("project.json"), &project)?;
        let first = workspace::build(&work, None)?;
        project["user_metadata"] = scalar;
        files::write_json(&work.join("project.json"), &project)?;
        let second = workspace::build(&work, None)?;
        assert_ne!(first["input_identity"], second["input_identity"]);
        assert_ne!(first["identity"], second["identity"]);
        assert_ne!(first["build"], second["build"]);
    }
    Ok(())
}

#[test]
fn literal_binary_markers_are_valid_metadata_objects() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), "Authored subtitle")?;
    let mut project = workspace::read_record(&work, "project.json")?;
    for value in [
        json!({"$bytes": [65]}),
        json!({"$set": [1]}),
        json!({"$num": "literal metadata"}),
    ] {
        project["user_metadata"] = value;
        files::write_json(&work.join("project.json"), &project)?;
        workspace::assemble(&work, None)?;
    }
    Ok(())
}

#[test]
fn bridge_dictionary_honors_requested_sort_order() -> Check {
    let value = json!({"$dict": [["b", 1], ["a", {"$dict": [["z", 3], ["y", 4]]}]]});
    assert_eq!(
        contract_json::dump(&value, true, false)?,
        r#"{"a": {"y": 4, "z": 3}, "b": 1}"#
    );
    assert_eq!(
        contract_json::dump(&value, false, false)?,
        r#"{"b": 1, "a": {"z": 3, "y": 4}}"#
    );
    Ok(())
}

#[test]
fn utf32_encodings_import_exact_text_and_keep_original_bytes() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = SRT.replace("Authored subtitle", "سلام 🙂");
    let le = source
        .chars()
        .flat_map(|c| (c as u32).to_le_bytes())
        .collect::<Vec<_>>();
    let be = source
        .chars()
        .flat_map(|c| (c as u32).to_be_bytes())
        .collect::<Vec<_>>();
    for (i, (label, bytes)) in [
        ("utf-32-le", le.clone()),
        ("utf_32_le", le.clone()),
        ("utf-32-be", be.clone()),
        ("utf-32", [&[0xff, 0xfe, 0, 0][..], &le].concat()),
        ("utf-32", [&[0, 0, 0xfe, 0xff][..], &be].concat()),
    ]
    .into_iter()
    .enumerate()
    {
        let input = root.path().join(format!("input-{i}.srt"));
        let work = root.path().join(format!("work-{i}"));
        fs::write(&input, &bytes).map_err(|e| e.to_string())?;
        assert_eq!(
            validation::text_encoding(json!(label), "encoding".into())?,
            label
        );
        prepare(&input, &work, label)?;
        assert_eq!(
            workspace::load(&work)?.documents["s0001"].cues[0].text,
            "سلام 🙂"
        );
        assert_eq!(
            fs::read(work.join("sources/s0001.srt")).map_err(|e| e.to_string())?,
            bytes
        );
    }
    Ok(())
}

#[test]
fn utf32_rejects_partial_units_surrogates_out_of_range_and_missing_bom() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let ordinary = SRT
        .chars()
        .flat_map(|c| (c as u32).to_le_bytes())
        .collect::<Vec<_>>();
    let mut cases = vec![
        ("utf-32", ordinary.clone()),
        ("utf-32-le", [ordinary.as_slice(), &[0]].concat()),
    ];
    for value in [0xd800_u32, 0xdfff, 0x110000] {
        cases.push((
            "utf-32-le",
            [ordinary.as_slice(), &value.to_le_bytes()].concat(),
        ));
    }
    for (i, (label, bytes)) in cases.into_iter().enumerate() {
        let input = root.path().join(format!("invalid-{i}.srt"));
        let work = root.path().join(format!("work-{i}"));
        fs::write(&input, bytes).map_err(|e| e.to_string())?;
        assert!(prepare(&input, &work, label).is_err());
        assert!(!work.exists());
    }
    Ok(())
}

#[test]
fn latin1_aliases_do_not_silently_select_windows1252() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("input.srt");
    let mut bytes = SRT.replace("Authored subtitle", "BYTE").into_bytes();
    let at = bytes
        .windows(4)
        .position(|part| part == b"BYTE")
        .ok_or("byte location")?;
    bytes.splice(at..at + 4, [0x80, 0x91, 0xff]);
    fs::write(&input, &bytes).map_err(|e| e.to_string())?;
    for (i, label) in [
        "latin1",
        "latin-1",
        "iso-8859-1",
        "iso8859-1",
        "iso8859_1",
        "ISO_8859-1",
    ]
    .into_iter()
    .enumerate()
    {
        let work = root.path().join(format!("latin-{i}"));
        prepare(&input, &work, label)?;
        assert_eq!(
            workspace::load(&work)?.documents["s0001"].cues[0].text,
            "\u{80}\u{91}ÿ",
            "{label}"
        );
    }
    let work = root.path().join("windows");
    prepare(&input, &work, "cp1252")?;
    assert_eq!(
        workspace::load(&work)?.documents["s0001"].cues[0].text,
        "€‘ÿ"
    );
    Ok(())
}

#[test]
fn unsupported_codecs_fail_validation_before_any_workspace_parent_creation() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("input.srt");
    fs::write(&input, SRT).map_err(|e| e.to_string())?;
    for label in [
        "utf-7",
        "unicode_escape",
        "raw_unicode_escape",
        "cp437",
        "cp850",
        "cp720",
        "cp864",
        "base64",
        "x-user-defined",
        "replacement",
    ] {
        assert!(
            validation::text_encoding(json!(label), "encoding".into()).is_err(),
            "{label}"
        );
        let parent = root.path().join(label);
        assert!(prepare(&input, &parent.join("work"), label).is_err());
        assert!(!parent.exists());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_work_destination_is_refused_before_publication() -> Check {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("input.srt");
    fs::write(&input, SRT).map_err(|e| e.to_string())?;
    let parent = root.path().join("not-created");
    let work = parent.join(std::ffi::OsString::from_vec(b"work-\xff".to_vec()));
    for _ in 0..2 {
        let error = prepare(&input, &work, "utf-8").expect_err("non-UTF8 work path");
        assert!(error.contains("UTF-8"), "{error}");
        assert!(!work.exists());
        assert!(!parent.exists());
    }
    Ok(())
}

#[test]
fn pending_workspace_rejects_a_present_invalid_output_format() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = root.path().join("input.srt");
    let work = root.path().join("work");
    fs::write(&input, SRT).map_err(|e| e.to_string())?;
    prepare(&input, &work, "utf-8")?;
    let mut project = workspace::read_record(&work, "project.json")?;
    for value in [json!("vtt"), json!(""), json!(1), json!(null)] {
        project["output_format"] = value;
        files::write_json(&work.join("project.json"), &project)?;
        assert!(validation::project(project.clone()).is_err());
        assert!(status::report(&work).is_err());
        assert!(workspace::load(&work).is_err());
    }
    project
        .as_object_mut()
        .ok_or("project object")?
        .remove("output_format");
    files::write_json(&work.join("project.json"), &project)?;
    assert_eq!(status::report(&work)?["state"], "pending");
    Ok(())
}

#[test]
fn plain_json_dump_preserves_every_marker_looking_object() -> Check {
    let value = json!({
        "$tuple": {"$int": "not an integer"},
        "nested": [{"$tuple": [1]}, {"$dict": [["b", 1], ["a", 2]]}, {"$bytes": [65]},
            {"$float": "not a float"}, {"$set": [1]}, {"$num": "-0"}],
        "ordinary": [2.0, 1e-5, "سلام 🙂"]
    });
    for sorted in [false, true] {
        for ascii in [false, true] {
            let encoded = contract_json::dump_json(&value, sorted, ascii)?;
            assert_eq!(
                serde_json::from_str::<Value>(&encoded).map_err(|e| e.to_string())?,
                value
            );
        }
    }
    Ok(())
}

#[test]
fn sorted_bridge_numeric_keys_keep_exact_integer_order() -> Check {
    let value = json!({"$dict": [[9007199254740993_u64, "larger"], [9007199254740992_u64, "smaller"], [-1.5, "fraction"], [false, "zero"], [-1, "negative"]]});
    assert_eq!(
        contract_json::dump(&value, true, false)?,
        r#"{"-1.5": "fraction", "-1": "negative", "false": "zero", "9007199254740992": "smaller", "9007199254740993": "larger"}"#
    );
    assert!(contract_json::dump(&json!({"$dict": [["a", 1], [2, 3]]}), true, false).is_err());
    assert!(contract_json::dump(&json!({"$dict": [["a"]]}), false, false).is_err());
    Ok(())
}

#[test]
fn public_donor_merge_commits_base_styles_and_keeps_failure_atomic() -> Check {
    use revayat_core::{
        subtitle_formats::{
            conversion::{self, Options},
            document::{self, Cue, Document},
        },
        workflow::{
            self,
            contract::{decode, marked},
        },
    };
    let options = Options {
        direction: "ltr",
        fix_zwnj: false,
        wrap_lines: false,
        strip_watermarks: false,
        ..Default::default()
    };
    let (base, _) = conversion::convert(&document::parse(SRT, "srt")?, &options)?;
    let mut donor = base.clone();
    donor.cues[0].start = 2000;
    donor.cues[0].end = 3000;
    donor.cues[0].text = r"{\rDefault}Donor".into();
    let mut marked_base = marked("Document", &base)?;
    let merged = workflow::merge_donor(
        &mut marked_base,
        marked("Document", &donor)?,
        vec![marked("Cue", &donor.cues[0])?],
        "s0002".into(),
        true,
    )?;
    let mut updated: Document = decode("Document", marked_base.clone(), "styles")?;
    assert!(updated.styles.contains_key("s0002__Default"));
    let cue: Cue = decode("Cue", merged[0].clone(), "fields")?;
    assert_eq!(cue.fields["style"], "s0002__Default");
    assert_eq!(cue.text, r"{\rs0002__Default}Donor");
    updated.cues.push(cue);
    document::serialize(&updated, &updated.cues, false)?;
    let before = marked_base.clone();
    donor.cues[0].fields.insert("style".into(), "Absent".into());
    assert!(
        workflow::merge_donor(
            &mut marked_base,
            marked("Document", &donor)?,
            vec![marked("Cue", &donor.cues[0])?],
            "s0003".into(),
            true
        )
        .is_err()
    );
    assert_eq!(marked_base, before);
    Ok(())
}

#[cfg(unix)]
#[test]
fn path_markers_and_inventory_refuse_lossy_filename_conversion() -> Check {
    use revayat_core::workflow::{self, contract::path_marker};
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root
        .path()
        .join(std::ffi::OsString::from_vec(b"authored-\xff.srt".to_vec()));
    fs::write(&path, SRT).map_err(|e| e.to_string())?;
    assert!(path_marker(&path).is_err_and(|e| e.contains("UTF-8")));
    assert!(
        workflow::input_candidates(path_marker(root.path())?, None)
            .is_err_and(|e| e.contains("UTF-8"))
    );
    let valid = root.path().join("سلام.srt");
    assert_eq!(
        path_marker(&valid)?["$path"],
        valid.to_str().ok_or("valid UTF-8")?
    );
    Ok(())
}

#[test]
fn recipe_upgrade_preserves_the_previous_generation_and_requires_fresh_evidence() -> Check {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let work = reviewed(root.path(), "Authored subtitle")?;
    let edition = workspace::assemble(&work, None)?;
    let old_recipe = json!({"version": 13, "normalization": 12, "timing": "floor-centisecond"});
    let old_id = workspace::digest(
        contract_json::dump(
            &json!([edition.manifest["input_identity"], old_recipe, "srt"]),
            true,
            true,
        )?
        .as_bytes(),
    );
    let old_dir = work.join("builds").join(&old_id);
    fs::create_dir_all(old_dir.join("Sub")).map_err(|e| e.to_string())?;
    fs::create_dir(old_dir.join("renders")).map_err(|e| e.to_string())?;
    let mut manifest = edition.manifest.clone();
    manifest["identity"] = json!(old_id);
    manifest["generation_recipe"] = old_recipe;
    files::write_json(&old_dir.join("manifest.json"), &manifest)?;
    files::write_json(&old_dir.join("glossary.json"), &edition.glossary)?;
    for (name, bytes) in &edition.files {
        fs::write(old_dir.join("Sub").join(name), bytes).map_err(|e| e.to_string())?;
    }
    fs::write(
        old_dir.join("renders/old-review.json"),
        b"authored prior approval marker",
    )
    .map_err(|e| e.to_string())?;
    let protected = [
        work.join("project.json"),
        work.join("sources/s0001.srt"),
        work.join("worksheets/s0001.json"),
        work.join("glossary.json"),
        old_dir.join("manifest.json"),
        old_dir.join("glossary.json"),
        old_dir.join("Sub/S01E01.srt"),
        old_dir.join("renders/old-review.json"),
    ];
    let before = protected
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let built = workspace::build(&work, None)?;
    assert_ne!(built["identity"], old_id);
    assert_eq!(built["input_identity"], edition.manifest["input_identity"]);
    assert_eq!(
        built["generation_recipe"],
        json!({"version": 14, "normalization": 13, "timing": "floor-centisecond"})
    );
    assert!(
        !Path::new(built["build"].as_str().ok_or("build path")?)
            .join("renders")
            .exists()
    );
    let after = protected
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    assert_eq!(after, before);
    Ok(())
}
