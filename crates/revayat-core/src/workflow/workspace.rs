use super::contract::{io_error, items, linked, string, value_error};
use super::review::{donor, reviewed_cues, verify_glossary};
use crate::publication::{publish_bytes, rename_noreplace};
pub use crate::runtime::digest_bytes as digest;
use crate::runtime::files::{local_path, read_json, read_limited, write_json};
use crate::subtitle_formats::document::{
    Document, ass_template, parse, presentation_order, serialize, srt_to_ass,
};
use crate::validation;
use regex::Regex;
#[cfg(unix)]
use rustix::{
    fd::OwnedFd,
    fs::{AtFlags, CWD, Dir, Mode, OFlags, fstat, mkdirat, openat, statat, unlinkat},
    io::{Errno, Result as UnixResult},
};
#[cfg(not(unix))]
use same_file::Handle;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub(super) const MAX_CUES: usize = 250_000;
pub(super) const MAX_FILE: u64 = 16 * 1024 * 1024;
pub(super) const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_JSON: u64 = 64 * 1024 * 1024;
pub(super) const MAX_WORKSPACE: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeVersion {
    pub normalization: u8,
    pub generation: u8,
}

pub const RECIPE_VERSION: RecipeVersion = RecipeVersion {
    normalization: 13,
    generation: 14,
};

pub fn utf8_path(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| value_error("Path must use valid UTF-8"))
}

pub fn display_path(path: &Path) -> Result<std::borrow::Cow<'_, str>, String> {
    let path = utf8_path(path)?;
    #[cfg(windows)]
    if let Some(value) = path.strip_prefix(r"\\?\") {
        if let Some(unc) = value.strip_prefix(r"UNC\") {
            return Ok(format!(r"\\{unc}").into());
        }
        if value.as_bytes().get(1..3) == Some(b":\\") {
            return Ok(value.into());
        }
    }
    Ok(path.into())
}

#[derive(Debug)]
pub struct StagedDir {
    path: PathBuf,
    #[cfg(unix)]
    original: Option<(OwnedFd, OwnedFd)>,
    #[cfg(not(unix))]
    original: Option<Handle>,
}

impl StagedDir {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn keep(mut self) -> PathBuf {
        self.original = None;
        std::mem::take(&mut self.path)
    }
}

impl AsRef<Path> for StagedDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Drop for StagedDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some((parent, original)) = &self.original
            && let Some(name) = self.path.file_name()
        {
            let _ = remove_staging(parent, name, original);
        }
        #[cfg(not(unix))]
        if let Some(original) = &self.original
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| metadata.is_dir() && !linked(&metadata))
            && Handle::from_path(&self.path).is_ok_and(|current| &current == original)
        {
            // shortcut: cleanup uses the path after verification; use relative handles when available.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(unix)]
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[cfg(unix)]
fn remove_staging(
    parent: &OwnedFd,
    name: impl rustix::path::Arg + Copy,
    dir: &OwnedFd,
) -> UnixResult<()> {
    let owned = fstat(dir)?;
    let unchanged = || {
        statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
            .is_ok_and(|entry| (entry.st_dev, entry.st_ino) == (owned.st_dev, owned.st_ino))
    };
    if !unchanged() {
        return Ok(());
    }
    for entry in Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        match openat(dir, name, DIR_FLAGS, Mode::empty()) {
            Ok(child) if fstat(&child)?.st_dev == owned.st_dev => {
                remove_staging(dir, name, &child)?;
            }
            Ok(_) => return Err(Errno::XDEV),
            Err(Errno::NOTDIR | Errno::LOOP) => unlinkat(dir, name, AtFlags::empty())?,
            Err(error) => return Err(error),
        }
    }
    if unchanged() {
        unlinkat(parent, name, AtFlags::REMOVEDIR)?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_staging(
    parent: &OwnedFd,
    name: &std::ffi::OsStr,
    open: impl FnOnce() -> UnixResult<OwnedFd>,
) -> UnixResult<OwnedFd> {
    mkdirat(parent, name, Mode::RWXU)?;
    open().inspect_err(|_| {
        let _ = unlinkat(parent, name, AtFlags::REMOVEDIR);
    })
}

#[cfg(unix)]
pub fn stage(parent: &Path, prefix: &str) -> Result<StagedDir, String> {
    let parent = std::path::absolute(parent).map_err(io_error)?;
    let parent_fd =
        openat(CWD, &parent, DIR_FLAGS, Mode::empty()).map_err(|error| io_error(error.into()))?;
    let stat = fstat(&parent_fd).map_err(|error| io_error(error.into()))?;
    let uid = rustix::process::geteuid().as_raw();
    // Trust owner-controlled parents, including root-owned sticky temporary directories.
    if stat.st_uid != uid && stat.st_uid != 0
        || stat.st_mode & 0o002 != 0 && stat.st_mode & 0o1000 == 0
    {
        return Err(value_error("Staging requires a protected parent directory"));
    }
    let temporary = tempfile::Builder::new()
        .prefix(prefix)
        .rand_bytes(6)
        .disable_cleanup(true)
        .make_in(&parent, |path| {
            let name = path
                .file_name()
                .filter(|_| path.parent() == Some(parent.as_path()))
                .ok_or(std::io::ErrorKind::InvalidInput)?;
            Ok(create_staging(&parent_fd, name, || {
                openat(&parent_fd, name, DIR_FLAGS, Mode::empty())
            })?)
        })
        .map_err(io_error)?;
    let (original, path) = temporary.keep().map_err(|error| io_error(error.error))?;
    Ok(StagedDir {
        path,
        original: Some((parent_fd, original)),
    })
}

#[cfg(not(unix))]
pub fn stage(parent: &Path, prefix: &str) -> Result<StagedDir, String> {
    let path = tempfile::Builder::new()
        .prefix(prefix)
        .rand_bytes(6)
        .tempdir_in(parent)
        .map_err(io_error)?
        .keep();
    let original = Handle::from_path(&path).map_err(io_error)?;
    Ok(StagedDir {
        path,
        original: Some(original),
    })
}

pub fn read_record(root: &Path, relative: &str) -> Result<Value, String> {
    read_json(&local_path(root, relative)?, MAX_JSON)
}

pub(super) fn matched<'a>(value: &'a Value, pattern: &str, at: &str) -> Result<&'a str, String> {
    let regex = Regex::new(pattern).map_err(value_error)?;
    value
        .as_str()
        .filter(|s| regex.is_match(s))
        .ok_or_else(|| value_error(format!("{at}: invalid identifier or digest")))
}

pub fn episode_identity(name: &str) -> Option<(i64, &str, i64)> {
    static PATTERN: LazyLock<Result<Regex, regex::Error>> =
        LazyLock::new(|| Regex::new(r"\AS([0-9]{2})(E|OVA)([0-9]{2}|[1-9][0-9]{2})\z"));
    let captures = PATTERN.as_ref().ok()?.captures(name)?;
    Some((
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str(),
        captures.get(3)?.as_str().parse().ok()?,
    ))
}

pub fn source_keys(episode: &Value) -> Vec<&str> {
    std::iter::once(string(&episode["base"]))
        .chain(items(&episode["alternates"]).iter().map(string))
        .collect()
}

pub fn pending(row: &Value) -> bool {
    row["reviewed"] != true || row["action"] == "edit" && row["text"].is_null()
}

pub type Documents = BTreeMap<String, Document>;
pub type Sheets = BTreeMap<String, Vec<Value>>;

pub struct Workspace {
    pub project: Value,
    pub documents: Documents,
    pub sheets: Sheets,
}

fn inventory(root: &Path, folder: &str) -> Result<BTreeSet<String>, String> {
    let mut result = BTreeSet::new();
    for entry in fs::read_dir(local_path(root, folder)?).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if result.len() >= 1000 || !metadata.is_file() || linked(&metadata) {
            return Err(value_error(
                "Workspace inventory exceeds its file limit or contains a non-file",
            ));
        }
        result.insert(utf8_path(Path::new(&entry.file_name()))?.into());
    }
    Ok(result)
}

pub fn load(work: &Path) -> Result<Workspace, String> {
    let project = validation::project(read_record(work, "project.json")?)?;
    let sources = items(&project["sources"]);
    let source_files = inventory(work, "sources")?;
    let worksheet_files = inventory(work, "worksheets")?;
    let expected_sources = sources
        .iter()
        .map(|s| format!("{}.{}", string(&s["id"]), string(&s["kind"])))
        .collect();
    let expected_sheets = sources
        .iter()
        .map(|s| format!("{}.json", string(&s["id"])))
        .collect();
    if source_files != expected_sources || worksheet_files != expected_sheets {
        return Err(value_error(
            "Source inventory differs from imported files or worksheets",
        ));
    }
    let (mut documents, mut sheets) = (Documents::new(), Sheets::new());
    let (mut source_bytes, mut expanded, mut cues) = (0, 0, 0);
    for source in sources {
        let key = matched(&source["id"], r"\As[0-9]{4}\z", "source.id")?;
        let kind = string(&source["kind"]);
        if documents.contains_key(key) {
            return Err(value_error("Invalid or duplicate source ID"));
        }
        let file = format!("sources/{key}.{kind}");
        if source["file"] != file {
            return Err(value_error(
                "Source path differs from its imported identity",
            ));
        }
        let raw = read_limited(&local_path(work, &file)?, MAX_FILE)?;
        source_bytes += raw.len() as u64;
        let expected_cues = source["cues"]
            .as_u64()
            .ok_or_else(|| value_error("Invalid source cue count"))?;
        cues += expected_cues;
        if source_bytes > MAX_TOTAL || cues > MAX_CUES as u64 {
            return Err(value_error("Workspace exceeds imported byte/cue limits"));
        }
        if digest(&raw) != source["sha256"] {
            return Err(value_error(
                "Source changed since import; create a fresh workspace",
            ));
        }
        let document = parse(
            &super::import::decode(&raw, string(&source["encoding"]))?,
            kind,
        )?;
        if document.cues.len() as u64 != expected_cues {
            return Err(value_error(format!(
                "project.json.sources.{key}.cues: source cue count differs"
            )));
        }
        let sheet_name = format!("worksheets/{key}.json");
        let sheet_path = local_path(work, &sheet_name)?;
        expanded += raw.len() as u64;
        let remaining = MAX_WORKSPACE
            .checked_sub(expanded)
            .ok_or_else(|| value_error("Expanded workspace exceeds the 512 MiB limit"))?;
        let sheet = validation::worksheet(read_json(&sheet_path, remaining)?, sheet_name)?;
        expanded += sheet_path.metadata().map_err(io_error)?.len();
        if expanded > MAX_WORKSPACE {
            return Err(value_error("Expanded workspace exceeds the 512 MiB limit"));
        }
        documents.insert(key.into(), document);
        sheets.insert(key.into(), sheet);
    }
    Ok(Workspace {
        project,
        documents,
        sheets,
    })
}

pub struct Edition {
    pub manifest: Value,
    pub files: BTreeMap<String, Vec<u8>>,
    pub glossary: Value,
}

pub fn format(output: &str) -> Result<(), String> {
    if matches!(output, "ass" | "srt") {
        Ok(())
    } else {
        Err(value_error("output_format must be ass or srt"))
    }
}

pub fn assemble(work: &Path, output_format: Option<&str>) -> Result<Edition, String> {
    let Workspace {
        project,
        documents: docs,
        sheets,
    } = load(work)?;
    let project_format = project.get("output_format").map_or("ass", string);
    format(project_format)?;
    let output = output_format.unwrap_or(project_format);
    format(output)?;
    let glossary = read_record(work, "glossary.json")?;
    verify_glossary(glossary.clone(), string(&project["series"]))?;
    let font_policy = string(&project["font_policy"]);
    if !matches!(font_policy, "keep" | "remove") {
        return Err(value_error("font_policy must be keep or remove"));
    }
    let episodes = items(&project["episodes"]);
    if episodes.is_empty() {
        return Err(value_error(
            "Map every source to an episode in project.json",
        ));
    }
    let language = string(&project["target_language"]);
    let reviewed = docs
        .iter()
        .map(|(key, doc)| Ok((key.as_str(), reviewed_cues(doc, &sheets[key], language)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let (mut assigned, mut names, mut files, mut report) =
        (Vec::new(), HashSet::new(), BTreeMap::new(), Vec::new());
    for episode in episodes {
        let name = string(&episode["id"]);
        let numbers = episode_identity(name).ok_or_else(|| {
            value_error(
                "Episode names require canonical ASCII SxxEyy or SxxOVAyy; use E01, not E001",
            )
        })?;
        if !names.insert(numbers) || project["season"] != numbers.0 {
            return Err(value_error(
                "Episode identities must be unique and match the project season",
            ));
        }
        if numbers.2 < 1 {
            return Err(value_error("Episode and OVA numbers start at 1"));
        }
        let sources = source_keys(episode);
        if sources.iter().any(|key| !docs.contains_key(*key))
            || string(&episode["comparison"]).trim().is_empty()
        {
            return Err(value_error(
                "Every episode needs valid sources and a comparison/selection reason",
            ));
        }
        let base_id = sources[0];
        if docs[base_id].kind != "ass" && sources.iter().any(|key| docs[*key].kind == "ass") {
            return Err(value_error(
                "Choose the ASS candidate as the base; content can come from other candidates",
            ));
        }
        assigned.extend(sources.iter().copied());
        let mut base = docs[base_id].clone();
        if base.kind == "ass" && output == "srt" {
            return Err(value_error(
                "Cannot produce SRT from an ASS base without losing its presentation",
            ));
        }
        let upgrade = base.kind == "srt" && output == "ass";
        if upgrade {
            base = ass_template(None, "", 1920, 1080)?;
        }
        let retained: HashSet<_> = sources
            .iter()
            .flat_map(|key| {
                reviewed[key]
                    .0
                    .iter()
                    .map(move |cue| format!("{key}:{}", cue.id))
            })
            .collect();
        let mut watermarks = Vec::new();
        let mut paired = Vec::new();
        for &key in &sources {
            let (cues, decisions) = &reviewed[key];
            for decision in decisions {
                let link_id = format!("{key}:{}", string(&decision["id"]));
                let links = items(&decision["links"]);
                if links.iter().any(|link| *link == link_id) {
                    return Err(value_error("A review link cannot target itself"));
                }
                if links.iter().any(|link| !retained.contains(string(link))) {
                    return Err(value_error(
                        "Review links must identify retained cues in this episode",
                    ));
                }
            }
            let original: BTreeMap<_, _> = docs[key]
                .cues
                .iter()
                .map(|cue| (&cue.id, [cue.start, cue.end]))
                .collect();
            let reviewed_times: BTreeMap<_, _> = cues
                .iter()
                .map(|cue| (cue.id.as_str(), [cue.start, cue.end]))
                .collect();
            let rows: BTreeMap<_, _> = sheets[key]
                .iter()
                .map(|row| (string(&row["id"]), row))
                .collect();
            let merged = if key != base_id && !upgrade {
                donor(
                    &mut base,
                    &docs[key],
                    cues.clone(),
                    key,
                    font_policy == "keep",
                )?
            } else {
                cues.clone()
            };
            for mut cue in merged {
                (cue.start, cue.end) = crate::subtitle_formats::document::effective_times(
                    cue.start, cue.end, &base.kind,
                )?;
                if upgrade {
                    let original_text = cue.text.clone();
                    cue = srt_to_ass(cue)?;
                    let reason = crate::text_pipeline::watermark_reason_with_kind(
                        original_text,
                        &docs[key].kind,
                    )?;
                    if !reason.is_empty() {
                        watermarks.push(json!({"source": key, "cue": cue.id, "reason": reason}));
                        continue;
                    }
                    let direction = rows[cue.id.as_str()]["direction"]
                        .as_str()
                        .unwrap_or("auto");
                    let direction = if direction == "auto" {
                        crate::markup::paragraph_direction(language.into())?
                    } else {
                        direction.into()
                    };
                    if direction == "rtl" {
                        cue.text = crate::text_pipeline::normalize_persian_zwnj(cue.text)?;
                    }
                    cue.text =
                        crate::text_pipeline::wrap_subtitle_lines(cue.text, Some(42), Some(2))?;
                    cue.text = crate::text_pipeline::anchor_rtl_line(
                        cue.text,
                        Some("ass".into()),
                        Some(direction),
                    )?;
                }
                let emitted = [cue.start, cue.end];
                let times = reviewed_times[cue.id.as_str()];
                let row = rows[cue.id.as_str()];
                let record = json!({"source": key, "cue": cue.id, "structure_changed": !string(&row["structure_note"]).trim().is_empty(), "timing": {"original": original[&cue.id], "reviewed": times, "emitted": emitted, "conversion": (emitted != times).then_some("floor-centisecond")}});
                paired.push((cue, record));
            }
        }
        let emitted: HashSet<_> = paired
            .iter()
            .map(|(_, row)| format!("{}:{}", string(&row["source"]), string(&row["cue"])))
            .collect();
        for key in &sources {
            if reviewed[key]
                .1
                .iter()
                .flat_map(|decision| items(&decision["links"]))
                .any(|link| !emitted.contains(string(link)))
            {
                return Err(value_error(
                    "Review links must identify emitted cues; a linked target was removed during normalization",
                ));
            }
        }
        let (cues, provenance): (Vec<_>, Vec<_>) = paired.into_iter().unzip();
        let order = presentation_order(&cues, &base.kind)?;
        let merged: Vec<_> = order.iter().map(|&index| cues[index].clone()).collect();
        let provenance: Vec<_> = order
            .iter()
            .enumerate()
            .map(|(i, &index)| {
                let mut record = json!({"emitted_index": i + 1});
                if let (Some(to), Some(from)) =
                    (record.as_object_mut(), provenance[index].as_object())
                {
                    to.extend(from.clone());
                }
                record
            })
            .collect();
        let payload = serialize(&base, &merged, font_policy == "remove")?;
        let check = parse(&payload, &base.kind)?;
        if check.cues.len() != merged.len() {
            return Err(value_error("Output cue count changed during serialization"));
        }
        let filename = format!("{name}.{}", base.kind);
        let mut episode = json!({"id": name, "file": format!("Sub/{filename}"), "sha256": digest(payload.as_bytes()), "cues": check.cues.len(), "sources": sources, "comparison": episode["comparison"], "provenance": provenance});
        if !watermarks.is_empty() {
            episode["watermarks_removed"] = json!(watermarks);
        }
        files.insert(filename, payload.into_bytes());
        report.push(episode);
    }
    assigned.sort_unstable();
    if !assigned.into_iter().eq(docs.keys().map(String::as_str)) {
        return Err(value_error("Assign each source to exactly one episode"));
    }
    let recipe = json!({"version": RECIPE_VERSION.generation, "normalization": RECIPE_VERSION.normalization, "timing": "floor-centisecond"});
    let raw = json!([project, sheets, glossary]);
    let input_identity = digest(super::contract_json::dump_json(&raw, true, false)?.as_bytes());
    let identity = digest(
        super::contract_json::dump_json(&json!([input_identity, recipe, output]), true, true)?
            .as_bytes(),
    );
    let origins: Map<_, _> = items(&project["sources"])
        .iter()
        .map(|source| {
            (
                string(&source["id"]).into(),
                source
                    .get("origin")
                    .cloned()
                    .unwrap_or_else(|| json!({"legacy_name": source["name"], "status": "legacy origin unavailable"})),
            )
        })
        .collect();
    let manifest = json!({"version": 2,"identity":identity,"input_identity":input_identity,"generation_recipe":recipe,"series":project["series"],"source_origins":origins,"target_language":language,"font_policy":font_policy,"output_format":output,"episodes":report,"reviewed_cues":docs.values().map(|doc|doc.cues.len()).sum::<usize>()});
    Ok(Edition {
        manifest,
        files,
        glossary,
    })
}

pub fn build(work: &Path, output_format: Option<&str>) -> Result<Value, String> {
    let Edition {
        manifest,
        files,
        glossary,
    } = assemble(work, output_format)?;
    let destination = local_path(work, &format!("builds/{}", string(&manifest["identity"])))?;
    if destination.exists() {
        let old = read_record(&destination, "manifest.json")?;
        if !crate::runtime::same_json(read_record(&destination, "glossary.json")?, glossary)? {
            return Err(value_error(
                "Existing build glossary differs from the reviewed glossary",
            ));
        }
        if !crate::runtime::same_json(old, manifest.clone())? {
            return Err(value_error(
                "Existing build has been modified; restore it or create a fresh workspace",
            ));
        }
        for (name, expected) in files {
            let bytes = local_path(&destination, &format!("Sub/{name}"))
                .and_then(|path| read_limited(&path, expected.len() as u64));
            let bytes = bytes.map_err(|err| {
                err.strip_prefix("ValueError: ").map_or_else(
                    || err.clone(),
                    |msg| value_error(format!("Existing build has been modified: {msg}")),
                )
            })?;
            if bytes != expected {
                return Err(value_error(
                    "Existing build has been modified; restore it or create a fresh workspace",
                ));
            }
        }
    } else {
        let parent = destination
            .parent()
            .ok_or_else(|| value_error("Edition has no parent directory"))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let staging = stage(parent, ".build-")?;
        let edition = staging.path().join("edition");
        let sub = edition.join("Sub");
        fs::create_dir_all(&sub).map_err(io_error)?;
        for (name, bytes) in files {
            publish_bytes(&sub.join(name), &bytes).map_err(io_error)?;
        }
        write_json(&edition.join("manifest.json"), &manifest)?;
        write_json(&edition.join("glossary.json"), &glossary)?;
        rename_noreplace(&edition, &destination).map_err(io_error)?;
    }
    let mut result = json!({"build": display_path(&destination)?});
    if let (Some(result), Value::Object(manifest)) = (result.as_object_mut(), manifest) {
        result.extend(manifest);
    }
    Ok(result)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn failed_staging_open_removes_the_empty_directory_and_preserves_the_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let parent = openat(CWD, root.path(), DIR_FLAGS, Mode::empty())?;
        let name = std::ffi::OsStr::new("staging");
        let result = create_staging(&parent, name, || {
            assert!(root.path().join(name).is_dir());
            Err(Errno::ACCESS)
        });
        assert!(matches!(result, Err(Errno::ACCESS)));
        assert!(matches!(
            statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW),
            Err(Errno::NOENT)
        ));
        Ok(())
    }
}
