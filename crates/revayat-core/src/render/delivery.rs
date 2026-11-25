use super::sampling::{
    MAX_RENDER_FRAMES, SAMPLER_VERSION, changed_indices, requested_fonts, samples,
};
use crate::png_validation::{MAX_PNG_BYTES, decode_dimensions};
use crate::publication::{create_dir_all_synced, publish_bytes};
use crate::runtime::files::{local_path, parse_json, read_limited, read_limited_with_metadata};
use crate::subtitle_formats::document::parse;
use crate::validation;
use crate::workflow::contract::{io_error, item, items, string, value_error};
#[cfg(windows)]
use crate::workflow::workspace::utf8_path;
use crate::workflow::workspace::{
    self, Documents, Edition, digest, display_path, read_record, stage,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, Instant};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MAX_SUBTITLE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_RENDERER_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_FONT_BYTES: u64 = 32 * 1024 * 1024;
pub const FRAME_FIELDS: &[&str] = &[
    "file",
    "time_seconds",
    "cue_indices",
    "sha256",
    "width",
    "height",
];
pub const SAMPLE_FIELDS: &[&str] = &["time_seconds", "cue_indices"];
pub const RECEIPT_FIELDS: &[&str] = &[
    "version",
    "identity",
    "episode",
    "subtitle_sha256",
    "background",
    "all_cues",
];

pub fn fields(value: &Value, keys: &[&str]) -> Result<Value, String> {
    keys.iter()
        .map(|&key| Ok((key.into(), item(value, key)?.clone())))
        .collect::<Result<Map<_, _>, _>>()
        .map(Value::Object)
}

pub fn subtitle_snapshot(build: &Path, episode: &Value) -> Result<Vec<u8>, String> {
    let path = local_path(build, string(item(episode, "file")?))?;
    let raw = read_limited(&path, MAX_SUBTITLE_BYTES)?;
    if digest(&raw) != *item(episode, "sha256")? {
        return Err(value_error(
            "Subtitle snapshot differs from the verified build; preserve sources and retry from a valid edition",
        ));
    }
    Ok(raw)
}

pub fn load_build(build: &Path) -> Result<(Value, Documents), String> {
    let manifest = validation::obj(read_record(build, "manifest.json")?, "manifest.json".into())?;
    match manifest["version"].as_i64() {
        None => {
            return Err(value_error(
                "manifest.json.version: expected an integer schema version",
            ));
        }
        Some(1) => {
            return Err(value_error(
                "Legacy build: run build --work on its workspace, then render and review the new edition; old artifacts remain unchanged",
            ));
        }
        Some(2) => (),
        Some(_) => {
            return Err(value_error(
                "manifest.json.version: unsupported build schema",
            ));
        }
    }
    let work = build
        .parent()
        .and_then(Path::parent)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let Edition {
        manifest: expected,
        files,
        glossary,
    } = workspace::assemble(work, manifest["output_format"].as_str())?;
    if !crate::runtime::same_json(manifest.clone(), expected.clone())? {
        return Err(value_error(
            "Build manifest or workspace modified; rebuild and review the new edition",
        ));
    }
    if build.file_name().and_then(|s| s.to_str()) != expected["identity"].as_str()
        || build
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            != Some("builds")
    {
        return Err(value_error(
            "Build must remain at its original workspace path",
        ));
    }
    if !crate::runtime::same_json(read_record(build, "glossary.json")?, glossary)? {
        return Err(value_error(
            "Build glossary differs from the reviewed continuity glossary",
        ));
    }
    let mut docs = Documents::new();
    for episode in items(&manifest["episodes"]) {
        let path = local_path(build, string(&episode["file"]))?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| value_error("Edition subtitle has no filename"))?;
        let expected = files
            .get(name)
            .ok_or_else(|| value_error("Edition subtitle is missing from its manifest"))?;
        let raw = read_limited(&path, expected.len() as u64)?;
        if &raw != expected || digest(&raw) != episode["sha256"] {
            return Err(value_error(
                "Built subtitle changed; rebuild from worksheets before rendering or delivery",
            ));
        }
        let kind = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let text = std::str::from_utf8(&raw).map_err(value_error)?;
        docs.insert(string(&episode["id"]).into(), parse(text, kind)?);
    }
    Ok((manifest, docs))
}

pub struct Delivery {
    pub manifest: Value,
    pub names: Vec<String>,
    pub backgrounds: BTreeSet<String>,
    pub frame_count: usize,
}

pub fn check_delivery(
    build: &Path,
    require_review: bool,
    selected: Option<&HashSet<&str>>,
) -> Result<Delivery, String> {
    let (manifest, docs) = load_build(build)?;
    let episodes: Vec<_> = items(&manifest["episodes"])
        .iter()
        .filter(|episode| selected.is_none_or(|ids| ids.contains(string(&episode["id"]))))
        .collect();
    let names: Vec<_> = episodes
        .iter()
        .map(|episode| string(&episode["file"]).to_owned())
        .collect();
    if names.iter().collect::<HashSet<_>>().len() != names.len() {
        return Err(value_error("Duplicate episode filenames"));
    }
    let (mut backgrounds, mut paths) = (BTreeSet::new(), HashSet::new());
    #[cfg(any(unix, windows))]
    let mut physical = HashSet::new();
    let mut frame_count = 0;
    let deadline = Instant::now() + Duration::from_secs(900);
    let renders = local_path(build, "renders")?;
    for episode in episodes {
        let id = string(&episode["id"]);
        let at = format!("renders/{id}.json");
        let evidence = validation::render_record(read_record(build, &at)?, at)?;
        if evidence["identity"] != manifest["identity"]
            || evidence["episode"] != episode["id"]
            || evidence["subtitle_sha256"] != episode["sha256"]
        {
            return Err(value_error("Render evidence is stale"));
        }
        let receipt_path = local_path(build, string(&evidence["receipt_file"]))?;
        let directory = receipt_path
            .parent()
            .ok_or_else(|| value_error("Render receipt is outside this episode's artifacts"))?;
        if receipt_path.file_name().and_then(|s| s.to_str()) != Some("receipt.json")
            || directory.parent() != Some(renders.as_path())
            || !directory
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(&format!("{id}-")))
        {
            return Err(value_error(
                "Render receipt is outside this episode's artifacts",
            ));
        }
        let raw = read_limited(&receipt_path, MAX_RECEIPT_BYTES)?;
        if digest(&raw) != evidence["receipt_sha256"] {
            return Err(value_error(
                "Render receipt changed; rerender rather than transferring approval",
            ));
        }
        let receipt = validation::obj(parse_json(&raw, "receipt.json")?, "render receipt".into())?;
        for &field in RECEIPT_FIELDS {
            if receipt[field] != evidence[field] {
                return Err(value_error(format!(
                    "Render receipt.{field}: association changed"
                )));
            }
        }
        let recipe = validation::obj(receipt["recipe"].clone(), "render receipt.recipe".into())?;
        if recipe["sampler"].as_i64() != Some(SAMPLER_VERSION) || recipe["profile"] != "rgb24-v1" {
            return Err(value_error(
                "Render sampling/profile recipe changed; rerender the edition",
            ));
        }
        let renderer = validation::fingerprint(
            recipe["renderer"].clone(),
            "render receipt.renderer".into(),
            Some(MAX_RENDERER_BYTES as i64),
        )?;
        validation::string(
            renderer["version"].clone(),
            "render receipt.renderer.version".into(),
            false,
        )?;
        let fonts = validation::array(recipe["fonts"].clone(), "render receipt.fonts".into(), 128)?;
        for (index, font) in fonts.into_iter().enumerate() {
            validation::fingerprint(
                font,
                format!("render receipt.fonts[{index}]"),
                Some(MAX_FONT_BYTES as i64),
            )?;
        }
        for font in validation::array(
            recipe["requested_fonts"].clone(),
            "render receipt.requested_fonts".into(),
            10_000,
        )? {
            validation::string(font, "render receipt.requested_fonts".into(), false)?;
        }
        let doc = &docs[id];
        if recipe["requested_fonts"] != json!(requested_fonts(doc)?) {
            return Err(value_error("Render font requests differ from the subtitle"));
        }
        let video = &recipe["video"];
        if !video.is_null() {
            validation::fingerprint(video.clone(), "render receipt.video".into(), None)?;
        }
        if evidence["background"] != if video.is_null() { "neutral" } else { "video" } {
            return Err(value_error(
                "Render background differs from its video provenance",
            ));
        }
        let expected = samples(
            doc,
            evidence["all_cues"] == true,
            &changed_indices(episode)?,
        )?;
        let receipt_samples = validation::array(
            receipt["samples"].clone(),
            "render receipt.samples".into(),
            MAX_RENDER_FRAMES as i64,
        )?;
        for sample in &receipt_samples {
            validation::obj(sample.clone(), "render receipt.sample".into())?;
            validation::number(
                sample["time_seconds"].as_f64().ok_or_else(|| {
                    value_error(
                        "render receipt.sample.time_seconds: expected a finite nonnegative number",
                    )
                })?,
                "render receipt.sample.time_seconds".into(),
            )?;
            for index in validation::array(
                sample["cue_indices"].clone(),
                "render receipt.sample.cue_indices".into(),
                100_000,
            )? {
                validation::integer(
                    index,
                    "render receipt.sample.cue_indices".into(),
                    1,
                    Some(100_000),
                )?;
            }
        }
        let immutable = validation::array(
            receipt["frames"].clone(),
            "render receipt.frames".into(),
            MAX_RENDER_FRAMES as i64,
        )?;
        let frames = items(&evidence["frames"]);
        if receipt_samples != expected
            || frames.len() != expected.len()
            || immutable.len() != expected.len()
        {
            return Err(value_error("Render coverage is incomplete"));
        }
        for (index, ((frame, original), sample)) in
            frames.iter().zip(immutable).zip(expected).enumerate()
        {
            if Instant::now() > deadline {
                return Err(value_error(
                    "Delivery QA exceeded its 900-second image budget",
                ));
            }
            validation::frame(
                original.clone(),
                format!("render receipt.frames[{index}]"),
                false,
            )?;
            if fields(frame, FRAME_FIELDS)? != original {
                return Err(value_error(
                    "Reviewed frame metadata differs from its immutable render receipt",
                ));
            }
            if fields(frame, SAMPLE_FIELDS)? != sample {
                return Err(value_error(
                    "Frame is not associated with the required sample",
                ));
            }
            let path = local_path(build, string(&frame["file"]))?;
            let name = format!("frame-{:04}.png", index + 1);
            if path.parent() != Some(directory)
                || path.file_name().and_then(|s| s.to_str()) != Some(name.as_str())
            {
                return Err(value_error(
                    "Frame path is not its assigned episode/sample file",
                ));
            }
            #[cfg(windows)]
            let key = utf8_path(&path)?.to_lowercase();
            #[cfg(not(windows))]
            let key = path.clone();
            if !paths.insert(key) {
                return Err(value_error("Duplicate or aliased render frame path"));
            }
            if require_review
                && (frame["reviewed"] != true || string(&frame["note"]).trim().is_empty())
            {
                return Err(value_error("Inspect every rendered frame before packaging"));
            }
            let (raw, metadata) = read_limited_with_metadata(&path, MAX_PNG_BYTES as u64)?;
            #[cfg(unix)]
            let identity = (metadata.dev(), metadata.ino());
            #[cfg(windows)]
            let identity = {
                use winsafe::co::{DISPOSITION, FILE_ATTRIBUTE, FILE_SHARE, GENERIC};
                let win_error = |error: winsafe::co::ERROR| {
                    io_error(std::io::Error::from_raw_os_error(error.raw() as i32))
                };
                let (file, _) = winsafe::HFILE::CreateFile(
                    utf8_path(&path)?,
                    GENERIC::READ,
                    Some(FILE_SHARE::READ | FILE_SHARE::WRITE | FILE_SHARE::DELETE),
                    None,
                    DISPOSITION::OPEN_EXISTING,
                    FILE_ATTRIBUTE::NORMAL,
                    None,
                    None,
                    None,
                )
                .map_err(win_error)?;
                let info = file.GetFileInformationByHandle().map_err(win_error)?;
                (info.dwVolumeSerialNumber, info.nFileIndex())
            };
            #[cfg(not(unix))]
            let _ = metadata;
            #[cfg(any(unix, windows))]
            if !physical.insert(identity) {
                return Err(value_error("Duplicate or aliased render frame path"));
            }
            if digest(&raw) != frame["sha256"] {
                return Err(value_error("Rendered image changed after review"));
            }
            let (width, height) = decode_dimensions(&raw)?;
            if frame["width"] != width || frame["height"] != height {
                return Err(value_error("PNG dimensions differ from the render receipt"));
            }
        }
        backgrounds.insert(string(&evidence["background"]).into());
        frame_count += frames.len();
    }
    Ok(Delivery {
        manifest,
        names,
        backgrounds,
        frame_count,
    })
}

pub fn qa(build: &Path) -> Result<Value, String> {
    let Delivery {
        manifest,
        names,
        backgrounds,
        frame_count,
    } = check_delivery(build, true, None)?;
    Ok(
        json!({"ok": true,"identity":manifest["identity"],"episodes":names.len(),"reviewed_cues":manifest["reviewed_cues"],"reviewed_frames":frame_count,"font_policy":manifest["font_policy"],"backgrounds":backgrounds,"sync_verified":false}),
    )
}

pub fn check_output(output: &Path) -> Result<(), String> {
    match fs::symlink_metadata(output) {
        Ok(_) => return Err(value_error("Output already exists; use a new ZIP filename")),
        Err(e) if e.kind() == ErrorKind::NotFound => (),
        Err(e) => return Err(io_error(e)),
    }
    if !output
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("zip"))
    {
        return Err(value_error("Final output must have a .zip extension"));
    }
    Ok(())
}

pub fn package(build: &Path, output: &Path) -> Result<Value, String> {
    use std::io::Write;
    let Delivery {
        manifest,
        names,
        backgrounds,
        ..
    } = check_delivery(build, true, None)?;
    check_output(output)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    create_dir_all_synced(parent).map_err(io_error)?;
    let temporary = stage(parent, ".package-")?;
    let path = temporary.path().join("edition.zip");
    let mut archive = ZipWriter::new(File::create_new(&path).map_err(io_error)?);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for episode in items(&manifest["episodes"]) {
        archive
            .start_file(string(&episode["file"]), options)
            .map_err(value_error)?;
        archive
            .write_all(&subtitle_snapshot(build, episode)?)
            .map_err(io_error)?;
    }
    drop(archive.finish().map_err(value_error)?);
    let mut archive = ZipArchive::new(File::open(&path).map_err(io_error)?).map_err(value_error)?;
    if archive.len() != names.len() {
        return Err(value_error("ZIP content validation failed"));
    }
    for (index, episode) in items(&manifest["episodes"]).iter().enumerate() {
        let entry = archive.by_index(index).map_err(value_error)?;
        if entry.name().map_err(value_error)? != names[index] {
            return Err(value_error("ZIP content validation failed"));
        }
        let mut raw = Vec::new();
        entry
            .take(MAX_SUBTITLE_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|_| value_error("ZIP content validation failed"))?;
        if raw.len() as u64 > MAX_SUBTITLE_BYTES || digest(&raw) != episode["sha256"] {
            return Err(value_error(
                "Packaged subtitle bytes differ from reviewed build",
            ));
        }
    }
    let bytes = fs::read(path).map_err(io_error)?;
    publish_bytes(output, &bytes).map_err(io_error)?;
    Ok(
        json!({"zip":display_path(output)?,"sha256":digest(&bytes),"episodes":names.len(),"glossary":display_path(&build.join("glossary.json"))?,"backgrounds":backgrounds,"sync_verified":false}),
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn package_refuses_to_clobber_even_a_dangling_symlink() -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(io_error)?;
        let output = temp.path().join("edition.zip");
        let target = temp.path().join("missing");
        std::os::unix::fs::symlink(&target, &output).map_err(io_error)?;
        assert!(check_output(&output).is_err());
        assert_eq!(fs::read_link(output).map_err(io_error)?, target);
        Ok(())
    }
}
