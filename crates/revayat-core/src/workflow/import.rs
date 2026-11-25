use super::contract::{io_error, linked, string, value_error};
use super::workspace::{MAX_CUES, MAX_FILE, MAX_TOTAL, MAX_WORKSPACE, digest, stage, utf8_path};
use crate::publication::{publish_bytes, rename_noreplace};
use crate::runtime::files::{read_json, write_json};
use crate::subtitle_formats::document::parse;
use crate::validation;
use encoding_rs::Encoding;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use zip::{CompressionMethod, ZipArchive};

fn resolve(path: &Path) -> Result<PathBuf, String> {
    match fs::canonicalize(path) {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let absolute = std::path::absolute(path).map_err(io_error)?;
            let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) else {
                return Ok(absolute);
            };
            let Some(name) = path.file_name() else {
                return Ok(absolute);
            };
            Ok(resolve(parent)?.join(name))
        }
        Err(e) => Err(io_error(e)),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn subtitle(ext: &str) -> bool {
    matches!(ext, "ass" | "srt")
}

pub fn input_candidates(path: &Path, excluded_root: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    let exclude = if let Some(owned) = excluded_root {
        let (owned, root) = (resolve(owned)?, resolve(path)?);
        if root.starts_with(&owned) {
            return Ok(Vec::new());
        }
        owned.strip_prefix(root).ok().map(|p| path.join(p))
    } else {
        None
    };
    if !path.is_dir() {
        return Ok(vec![path.to_owned()]);
    }
    let mut pending = vec![path.to_owned()];
    let mut found = Vec::new();
    let mut visited = 0;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if exclude.as_ref() == Some(&path) {
                continue;
            }
            visited += 1;
            if visited > 100_000 {
                return Err(value_error(
                    "Input traversal exceeds 100000 directory entries",
                ));
            }
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if linked(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file()
                && matches!(extension(&path).as_str(), "zip" | "ass" | "srt")
            {
                found.push(path);
                if found.len() > 1000 {
                    return Err(value_error("Too many candidate subtitle files or archives"));
                }
            }
        }
    }
    #[cfg(windows)]
    found.sort_by_cached_key(|path| path.to_string_lossy().to_lowercase());
    #[cfg(not(windows))]
    found.sort();
    Ok(found)
}

fn member_name(name: &str) -> Result<String, String> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|part| part == ".." || part.contains(':'))
    {
        return Err(value_error("Unsafe ZIP member path"));
    }
    Ok(normalized
        .split('/')
        .filter(|part| !matches!(*part, "." | ""))
        .collect::<Vec<_>>()
        .join("/"))
}

#[derive(Default)]
struct Budget {
    count: usize,
    bytes: u64,
}

impl Budget {
    fn claim(&mut self, size: u64, problem: &str) -> Result<(), String> {
        self.count += 1;
        if size > MAX_FILE || self.bytes.saturating_add(size) > MAX_TOTAL || self.count > 1000 {
            return Err(value_error(problem));
        }
        Ok(())
    }

    fn read(&mut self, input: impl Read, archive: bool) -> Result<Vec<u8>, String> {
        let mut raw = Vec::new();
        input
            .take(MAX_FILE + 1)
            .read_to_end(&mut raw)
            .map_err(|e| {
                if archive {
                    value_error(format!("Corrupt ZIP member compressed stream: {e}"))
                } else {
                    io_error(e)
                }
            })?;
        self.bytes += raw.len() as u64;
        if raw.len() as u64 > MAX_FILE || self.bytes > MAX_TOTAL {
            return Err(value_error(if archive {
                "Expanded subtitle data exceeds import limits"
            } else {
                "Subtitle grew beyond import limits while reading"
            }));
        }
        Ok(raw)
    }
}

fn inputs(
    paths: &[PathBuf],
    excluded: &Path,
    mut push: impl FnMut(String, &str, Vec<u8>, Value) -> Result<(), String>,
) -> Result<(), String> {
    if paths.len() > 1000 {
        return Err(value_error("Too many input roots"));
    }
    let mut budget = Budget::default();
    for (i, root) in paths.iter().enumerate() {
        if linked(&fs::symlink_metadata(root).map_err(io_error)?) {
            return Err(value_error(
                "Symbolic-link subtitle inputs are not accepted",
            ));
        }
        let root_label = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("filesystem-root");
        let root_id = format!("input{:03}", i + 1);
        for candidate in input_candidates(root, Some(excluded))? {
            let metadata = fs::symlink_metadata(&candidate).map_err(io_error)?;
            if linked(&metadata) {
                continue;
            }
            if !metadata.is_file() {
                return Err(value_error(
                    "Subtitle/archive input must be a regular file, not a pipe or device",
                ));
            }
            let relative = if root.is_dir() {
                candidate.strip_prefix(root).map_err(value_error)?
            } else {
                Path::new(
                    candidate
                        .file_name()
                        .ok_or_else(|| value_error("Expected a filename"))?,
                )
            };
            let relative = utf8_path(relative)?.replace(std::path::MAIN_SEPARATOR, "/");
            let mut origin = json!({"root_id": root_id, "root_label": root_label, "relative_path": relative, "member": null, "member_index": null});
            let name = format!("{root_id}/{relative}");
            let ext = extension(&candidate);
            if ext == "zip" {
                if metadata.len() > MAX_TOTAL {
                    return Err(value_error("ZIP input exceeds the 256 MiB archive limit"));
                }
                let mut archive = ZipArchive::new(File::open(&candidate).map_err(io_error)?)
                    .map_err(|err| format!("zipfile.BadZipFile: {err}"))?;
                if archive.len() > 10_000 {
                    return Err(value_error("Too many ZIP members"));
                }
                for index in 0..archive.len() {
                    let entry = archive.by_index_raw(index).map_err(value_error)?;
                    let entry_name = entry.name().map_err(value_error)?;
                    let path = member_name(&entry_name)?;
                    let kind = extension(Path::new(&path));
                    if !subtitle(&kind) || entry.is_dir() {
                        continue;
                    }
                    if entry
                        .unix_mode()
                        .is_some_and(|mode| mode & 0o170000 == 0o120000)
                    {
                        return Err(value_error("ZIP symbolic links are not accepted"));
                    }
                    if entry.encrypted() {
                        return Err(value_error(
                            "Encrypted ZIP: ask for the password and extract separately",
                        ));
                    }
                    if !matches!(
                        entry.compression(),
                        CompressionMethod::Stored
                            | CompressionMethod::Deflated
                            | CompressionMethod::Bzip2
                            | CompressionMethod::Lzma
                    ) {
                        return Err(value_error("Unsupported ZIP compression method"));
                    }
                    budget.claim(
                        entry.size(),
                        "Subtitle archive exceeds bounded import limits",
                    )?;
                    origin["member"] = json!(entry_name);
                    origin["member_index"] = json!(index + 1);
                    drop(entry);
                    let entry = archive
                        .by_index(index)
                        .map_err(|_| value_error("Unsupported or encrypted ZIP member"))?;
                    push(
                        format!("{name}!/{path}"),
                        &kind,
                        budget.read(entry, true)?,
                        origin.clone(),
                    )?;
                }
            } else if subtitle(&ext) {
                budget.claim(
                    metadata.len(),
                    "Subtitle input exceeds bounded import limits",
                )?;
                push(
                    name,
                    &ext,
                    budget.read(File::open(&candidate).map_err(io_error)?, false)?,
                    origin,
                )?;
            } else if !root.is_dir() {
                return Err(value_error(
                    "Expected ASS, SRT, ZIP, or a directory containing subtitles",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn decode(raw: &[u8], label: &str) -> Result<String, String> {
    let label = label.to_ascii_lowercase().replace('_', "-");
    let invalid = || format!("UnicodeDecodeError: invalid {label} subtitle data");
    match label.as_str() {
        "utf-8" | "utf8" | "utf-8-sig" => {
            let raw = if label == "utf-8-sig" {
                raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw)
            } else {
                raw
            };
            std::str::from_utf8(raw)
                .map(str::to_owned)
                .map_err(|_| invalid())
        }
        "ascii" | "us-ascii" if raw.is_ascii() => Ok(String::from_utf8_lossy(raw).into_owned()),
        "ascii" | "us-ascii" => Err(invalid()),
        "latin-1" | "latin1" | "iso-8859-1" => Ok(raw.iter().map(|&b| char::from(b)).collect()),
        "utf-16" | "utf16" => match raw {
            [0xff, 0xfe, tail @ ..] => decode(tail, "utf-16le"),
            [0xfe, 0xff, tail @ ..] => decode(tail, "utf-16be"),
            _ => Err(invalid()),
        },
        "utf-16-le" | "utf16-le" => decode(raw, "utf-16le"),
        "utf-16-be" | "utf16-be" => decode(raw, "utf-16be"),
        _ => Encoding::for_label(label.as_bytes())
            .or_else(|| {
                label
                    .strip_prefix("cp")
                    .and_then(|suffix| Encoding::for_label(format!("windows-{suffix}").as_bytes()))
            })
            .ok_or_else(|| {
                value_error("encoding: expected a supported text encoding, not a binary transform")
            })?
            .decode_without_bom_handling_and_without_replacement(raw)
            .map(|s| s.into_owned())
            .ok_or_else(invalid),
    }
}

pub struct ImportOptions<'a> {
    pub series: &'a str,
    pub season: i64,
    pub encoding: &'a str,
    pub glossary: Option<&'a Path>,
    pub language: &'a str,
    pub output_format: &'a str,
}

pub fn prepare(
    paths: &[PathBuf],
    work: &Path,
    options: &ImportOptions<'_>,
) -> Result<Value, String> {
    let ImportOptions {
        series,
        season,
        encoding,
        glossary,
        language,
        output_format,
    } = *options;
    super::workspace::format(output_format)?;
    validation::string(json!(series), "series".into(), false)?;
    validation::integer(json!(season), "season".into(), 1, Some(99))?;
    validation::text_encoding(json!(encoding), "encoding".into())?;
    if series.trim().is_empty() {
        return Err(value_error(
            "A series title and season from 1 to 99 are required",
        ));
    }
    if fs::symlink_metadata(work).is_ok() {
        return Err(value_error(
            "Work directory already exists; resume it or choose a new directory",
        ));
    }
    let terms = if let Some(path) = glossary {
        let glossary =
            validation::glossary(read_json(path, 64 * 1024 * 1024)?, "glossary.json".into())?;
        if string(&glossary["series"]) != series {
            return Err(value_error(
                "The previous glossary belongs to a different series identity",
            ));
        }
        glossary
    } else {
        json!({"series": series, "research": [], "terms": [], "terms_reviewed": false})
    };
    super::workspace::matched(
        &json!(language),
        r"\A[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*\z",
        "target_language",
    )?;
    let parent = work
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(io_error)?;
    let staging = stage(parent, ".subtitle-import-")?;
    let temporary = staging.path().join("work");
    fs::create_dir_all(temporary.join("sources")).map_err(io_error)?;
    fs::create_dir(temporary.join("worksheets")).map_err(io_error)?;
    let mut sources = Vec::new();
    let (mut total_cues, mut expanded) = (0, 0);
    inputs(paths, staging.path(), |name, kind, raw, origin| {
        let doc = parse(&decode(&raw, encoding)?, kind)?;
        total_cues += doc.cues.len();
        if total_cues > MAX_CUES {
            return Err(value_error("Workspace exceeds 250000 imported cues"));
        }
        let id = format!("s{:04}", sources.len() + 1);
        let relative = format!("sources/{id}.{kind}");
        publish_bytes(&temporary.join(&relative), &raw).map_err(io_error)?;
        let sheet: Vec<_> = doc.cues.iter().map(|cue| json!({"id": cue.id, "source_text": cue.text, "start_ms": cue.start, "end_ms": cue.end, "action": "edit", "text": null, "reviewed": false, "links": [], "note": "", "timing_note": "", "structure_note": ""})).collect();
        let worksheet = temporary.join("worksheets").join(format!("{id}.json"));
        write_json(&worksheet, &json!(sheet))?;
        expanded += raw.len() as u64 + fs::metadata(worksheet).map_err(io_error)?.len();
        if expanded > MAX_WORKSPACE {
            return Err(value_error("Expanded workspace exceeds the 512 MiB limit"));
        }
        sources.push(json!({"id": id, "name": name, "file": relative, "kind": kind, "sha256": digest(&raw), "encoding": encoding, "cues": doc.cues.len(), "comments_removed": doc.comments, "origin": origin}));
        Ok(())
    })?;
    if sources.is_empty() {
        return Err(value_error("No ASS/SRT subtitles found"));
    }
    let project = json!({"version": 2, "series": series, "season": season, "target_language": language, "font_policy": "keep", "output_format": output_format, "sources": sources, "episodes": []});
    write_json(&temporary.join("project.json"), &project)?;
    write_json(&temporary.join("glossary.json"), &terms)?;
    rename_noreplace(&temporary, work).map_err(io_error)?;
    Ok(
        json!({"work": utf8_path(work)?, "sources": sources, "next": "Read and fill every worksheet; map episodes in project.json"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traverses_without_reading_owned_stage() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(io_error)?;
        let root = dir.path();
        let stage = root.join(".stage");
        fs::create_dir(&stage).map_err(io_error)?;
        fs::write(stage.join("staged.ass"), []).map_err(io_error)?;
        fs::write(root.join("episode.SRT"), []).map_err(io_error)?;
        assert_eq!(
            input_candidates(root, Some(&stage))?,
            [root.join("episode.SRT")]
        );
        assert!(input_candidates(&stage, Some(&stage))?.is_empty());
        assert!(member_name("./safe/../episode.srt").is_err());
        assert!(member_name("C:\\episode.srt").is_err());
        assert_eq!(member_name("./safe//episode.srt")?, "safe/episode.srt");
        Ok(())
    }
}
