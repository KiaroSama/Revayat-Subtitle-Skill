use super::delivery::{
    FRAME_FIELDS, MAX_FONT_BYTES, MAX_RENDERER_BYTES, RECEIPT_FIELDS, SAMPLE_FIELDS, fields,
    load_build, subtitle_snapshot,
};
use super::sampling::{SAMPLER_VERSION, changed_indices, requested_fonts, samples};
use crate::png_validation::{MAX_PNG_BYTES, decode_dimensions};
use crate::process_control::{RunError, RunOptions, run_owned};
use crate::publication::publish_bytes;
use crate::runtime::files::{
    file_fingerprint, local_path, output_directory, read_limited, write_json,
};
use crate::subtitle_formats::document::Document;
use crate::workflow::contract::{io_error, item, items, linked, string, value_error};
use crate::workflow::workspace::{digest, display_path, stage, utf8_path};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_VIDEO_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MISSING: &str = "FFmpeg not found; install a libass-enabled build or set REVAYAT_FFMPEG";

pub use crate::runtime::assets::bundled_fonts_dir;

fn extension(path: &Path) -> Option<String> {
    path.extension()?.to_str().map(str::to_ascii_lowercase)
}

fn search_path() -> Option<PathBuf> {
    let search = env::var_os("PATH")?;
    #[cfg(windows)]
    let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    #[cfg(windows)]
    let names: Vec<_> = extensions
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|ext| format!("ffmpeg{ext}"))
        .collect();
    #[cfg(not(windows))]
    let names = ["ffmpeg"];
    for dir in env::split_paths(&search) {
        for name in &names {
            let path = dir.join(name);
            if let Ok(info) = path.metadata()
                && info.is_file()
            {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if info.permissions().mode() & 0o111 == 0 {
                        continue;
                    }
                }
                return Some(path);
            }
        }
    }
    None
}

pub fn ffmpeg_path(explicit: Option<&str>) -> Result<PathBuf, String> {
    let executable = explicit
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("REVAYAT_FFMPEG")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        })
        .or_else(search_path)
        .filter(|path| path.is_file())
        .ok_or_else(|| value_error(MISSING))?;
    absolute(&executable)
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    let path = path.canonicalize().map_err(io_error)?;
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(local) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(local));
        }
    }
    Ok(path)
}

fn run(args: &[String], cwd: Option<&Path>, timeout: Duration) -> Result<Vec<u8>, String> {
    run_owned(
        args,
        &RunOptions {
            cwd,
            timeout,
            ..RunOptions::default()
        },
    )
    .map(|output| output.stdout)
    .map_err(|error| match error {
        RunError::Timeout { .. } => format!("subprocess.TimeoutExpired: {}", error),
        RunError::Spawn(error) | RunError::Io(error) => io_error(error),
        _ => value_error(error),
    })
}

pub fn doctor(override_path: Option<&str>) -> Result<Value, String> {
    let attempt: Result<Value, String> = (|| {
        let executable = ffmpeg_path(override_path)?;
        let name = utf8_path(&executable)?;
        let bytes = run(
            &[name.into(), "-hide_banner".into(), "-filters".into()],
            None,
            Duration::from_secs(15),
        )?;
        let filters =
            std::str::from_utf8(&bytes).map_err(|e| format!("UnicodeDecodeError: {e}"))?;
        let available = |name: &str| {
            filters.match_indices(name).any(|(start, part)| {
                filters[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
                    && filters[start + part.len()..]
                        .chars()
                        .next()
                        .is_some_and(char::is_whitespace)
            })
        };
        let ready = ["ass", "subtitles", "color"].into_iter().all(available);
        Ok(
            json!({"ready": ready, "ffmpeg": name, "libass": ready, "font_check": "A successful render plus visual inspection is required to verify Persian glyphs"}),
        )
    })();
    match attempt {
        Ok(report) => Ok(report),
        Err(error) if error.starts_with("subprocess.TimeoutExpired: ") => Err(error),
        Err(error) => Ok(
            json!({"ready": false, "error": error.split_once(": ").map_or(error.as_str(), |(_, msg)| msg)}),
        ),
    }
}

pub struct RenderEvidence<'a> {
    pub manifest: &'a Value,
    pub episode: &'a Value,
    pub document: &'a Document,
    pub frames: &'a [Value],
    pub all_cues: bool,
    pub recipe: &'a Value,
}

pub fn write_evidence(build: &Path, evidence: &RenderEvidence<'_>) -> Result<Value, String> {
    let RenderEvidence {
        manifest,
        episode,
        document,
        frames,
        all_cues,
        recipe,
    } = *evidence;
    let path = local_path(
        build,
        &format!("renders/{}.json", string(item(episode, "id")?)),
    )?;
    if frames.is_empty() {
        return Err(value_error("A render must produce at least one frame"));
    }
    let immutable = frames
        .iter()
        .map(|frame| fields(frame, FRAME_FIELDS))
        .collect::<Result<Vec<_>, _>>()?;
    let samples = samples(document, all_cues, &changed_indices(episode)?)?;
    if frames
        .iter()
        .map(|frame| fields(frame, SAMPLE_FIELDS))
        .collect::<Result<Vec<_>, _>>()?
        != samples
    {
        return Err(value_error(
            "Rendered frames do not match their required cue samples",
        ));
    }
    let background = if recipe["video"].is_null() {
        "neutral"
    } else {
        "video"
    };
    let receipt = json!({"version":2,"identity":item(manifest, "identity")?,"episode":episode["id"],"subtitle_sha256":item(episode, "sha256")?,"all_cues":all_cues,"background":background,"recipe":recipe,"samples":samples,"frames":immutable});
    let first = string(&immutable[0]["file"]);
    let rel = Path::new(first)
        .parent()
        .ok_or_else(|| value_error("Frame has no render directory"))?
        .join("receipt.json");
    let relative = utf8_path(&rel)?.replace(std::path::MAIN_SEPARATOR, "/");
    let receipt_path = local_path(build, &relative)?;
    write_json(&receipt_path, &receipt)?;
    let raw = read_limited(&receipt_path, 16 * 1024 * 1024)?;
    let mut evidence = fields(&receipt, RECEIPT_FIELDS)?;
    evidence["receipt_file"] = json!(relative);
    evidence["receipt_sha256"] = json!(digest(&raw));
    evidence["frames"] = json!(
        immutable
            .into_iter()
            .map(|mut row| {
                row["reviewed"] = json!(false);
                row["note"] = json!("");
                row
            })
            .collect::<Vec<_>>()
    );
    write_json(&path, &evidence)?;
    Ok(
        json!({"review":display_path(&path)?,"frames":evidence["frames"],"next":"Inspect every image; set reviewed=true and write an observation for each frame"}),
    )
}

fn stage_fonts(fonts: &Path, stage: &Path) -> Result<Vec<Value>, String> {
    if !fonts.is_dir() {
        return Err(value_error(
            "fonts-dir must be a directory containing TTF/OTF files",
        ));
    }
    let target = stage.join("fonts");
    fs::create_dir(&target).map_err(io_error)?;
    let mut files = Vec::new();
    for (i, entry) in fs::read_dir(fonts).map_err(io_error)?.enumerate() {
        if i >= 10_000 {
            return Err(value_error("Font directory exceeds the entry budget"));
        }
        files.push(entry.map_err(io_error)?.path());
    }
    #[cfg(windows)]
    files.sort_by_cached_key(|path| path.to_string_lossy().to_lowercase());
    #[cfg(not(windows))]
    files.sort();
    let mut bytes = 0;
    let mut found = Vec::new();
    for path in files {
        if !extension(&path).is_some_and(|s| matches!(s.as_str(), "ttf" | "otf" | "ttc"))
            || !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && !linked(&m))
        {
            continue;
        }
        let info = file_fingerprint(&path, MAX_FONT_BYTES)?;
        bytes += info["bytes"]
            .as_u64()
            .ok_or_else(|| value_error("Invalid font fingerprint size"))?;
        if bytes > 64 * 1024 * 1024 || found.len() >= 128 {
            return Err(value_error(
                "Selected fonts exceed the 64 MiB/128-file budget",
            ));
        }
        let raw = read_limited(&path, MAX_FONT_BYTES)?;
        if info["bytes"] != raw.len() || info["sha256"] != digest(&raw) {
            return Err(value_error("Font changed during render staging"));
        }
        let name = path
            .file_name()
            .ok_or_else(|| value_error("Font has no filename"))?;
        let staged = target.join(name);
        publish_bytes(&staged, &raw).map_err(io_error)?;
        if file_fingerprint(&staged, MAX_FONT_BYTES)? != info {
            return Err(value_error("Font changed during render staging"));
        }
        found.push(info);
    }
    if found.is_empty() {
        return Err(value_error("fonts-dir contains no supported font files"));
    }
    Ok(found)
}

fn args_at(
    executable: &str,
    video: Option<&str>,
    seconds: &Value,
    filter: &str,
    filename: &str,
) -> Vec<String> {
    let mut command: Vec<_> = [
        executable,
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-threads",
        "1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let seconds = seconds.to_string();
    if let Some(video) = video {
        command.extend(
            ["-ss", &seconds, "-i", video]
                .into_iter()
                .map(str::to_owned),
        );
    } else {
        command.extend(
            ["-f", "lavfi", "-i", "color=c=0x202020:s=1280x720:r=1:d=1"]
                .into_iter()
                .map(str::to_owned),
        );
    }
    let filters = format!("settb=AVTB,setpts=PTS-STARTPTS+{seconds}/TB,{filter}");
    command.extend(
        [
            "-filter_threads",
            "1",
            "-vf",
            &filters,
            "-frames:v",
            "1",
            "-update",
            "1",
            "-pix_fmt",
            "rgb24",
            "-threads",
            "1",
            filename,
        ]
        .into_iter()
        .map(str::to_owned),
    );
    command
}

pub fn render(
    build: &Path,
    episode_id: &str,
    executable: Option<&str>,
    video: Option<&Path>,
    fonts: Option<&Path>,
    all_cues: bool,
) -> Result<Value, String> {
    let root = local_path(build, "renders")?;
    let ffmpeg = ffmpeg_path(executable)?;
    let executable = utf8_path(&ffmpeg)?;
    let (manifest, docs) = load_build(build)?;
    let doc = docs
        .get(episode_id)
        .ok_or_else(|| value_error("Episode ID is not in this build"))?;
    local_path(build, &format!("renders/{episode_id}.json"))?;
    let episode = items(&manifest["episodes"])
        .iter()
        .find(|ep| ep["id"] == episode_id)
        .ok_or_else(|| value_error("Episode ID is not in this build"))?;
    let required = samples(doc, all_cues, &changed_indices(episode)?)?;
    let deadline = Instant::now() + Duration::from_secs(900);
    let fingerprint = file_fingerprint(&ffmpeg, MAX_RENDERER_BYTES)?;
    let bytes = run(
        &[executable.into(), "-hide_banner".into(), "-version".into()],
        None,
        Duration::from_secs(15),
    )?;
    let version = std::str::from_utf8(&bytes)
        .map_err(value_error)?
        .lines()
        .next()
        .ok_or_else(|| "IndexError: list index out of range".to_owned())?
        .chars()
        .take(500)
        .collect::<String>();
    let mut renderer = fingerprint.clone();
    renderer["version"] = json!(version);
    let video_fingerprint = video
        .map(|path| file_fingerprint(path, MAX_VIDEO_BYTES))
        .transpose()?;
    let video_path = video.map(absolute).transpose()?;
    let video_arg = video_path.as_deref().map(utf8_path).transpose()?;
    let default_fonts = fonts.is_none().then(bundled_fonts_dir).flatten();
    let fonts = fonts.or(default_fonts.as_deref());
    let mut recipe = json!({"sampler":SAMPLER_VERSION,"profile":"rgb24-v1","renderer":renderer,"fonts":[],"requested_fonts":requested_fonts(doc)?,"font_resolution":"system fallback requires visual inspection; selected system font files are not attested","video":video_fingerprint});
    fs::create_dir_all(&root).map_err(io_error)?;
    let destination = output_directory(&root, &format!("{episode_id}-"))?;
    let staging = stage(build, ".render-")?;
    let stage = staging.path();
    let input = format!("input.{}", doc.kind);
    publish_bytes(&stage.join(&input), &subtitle_snapshot(build, episode)?).map_err(io_error)?;
    let mut filter = if doc.kind == "ass" {
        format!("ass={input}:shaping=complex")
    } else {
        format!("subtitles={input}")
    };
    if let Some(fonts) = fonts {
        recipe["fonts"] = json!(stage_fonts(fonts, stage)?);
        filter.push_str(":fontsdir=fonts");
    }
    let relative = destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| value_error("Render directory has no filename"))?;
    let mut frames = Vec::new();
    let mut bytes = 0;
    for (i, mut sample) in required.into_iter().enumerate() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(value_error(
                "Render exceeded its 900-second episode budget; existing evidence remains intact",
            ));
        }
        let filename = format!("frame-{:04}.png", i + 1);
        let command = args_at(
            executable,
            video_arg,
            &sample["time_seconds"],
            &filter,
            &filename,
        );
        run(
            &command,
            Some(stage),
            remaining.min(Duration::from_secs(45)),
        )?;
        let path = stage.join(&filename);
        if !fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() >= 100) {
            return Err(value_error(
                "FFmpeg produced no image at the requested time",
            ));
        }
        let raw = read_limited(&path, MAX_PNG_BYTES as u64)?;
        let (width, height) = decode_dimensions(&raw)?;
        bytes += raw.len() as u64;
        if bytes > 2 * 1024 * 1024 * 1024 {
            return Err(value_error(
                "Render exceeded its 2 GiB episode artifact budget",
            ));
        }
        publish_bytes(&destination.join(&filename), &raw).map_err(io_error)?;
        sample["file"] = json!(format!("renders/{relative}/{filename}"));
        sample["sha256"] = json!(digest(&raw));
        sample["width"] = json!(width);
        sample["height"] = json!(height);
        frames.push(sample);
    }
    if let Some(path) = video
        && file_fingerprint(path, MAX_VIDEO_BYTES)? != recipe["video"]
    {
        return Err(value_error("Video changed during rendering"));
    }
    if file_fingerprint(&ffmpeg, MAX_RENDERER_BYTES)? != fingerprint {
        return Err(value_error("Renderer changed during rendering"));
    }
    if Instant::now() > deadline {
        return Err(value_error("Render exceeded its 900-second episode budget"));
    }
    load_build(build)?;
    write_evidence(
        build,
        &RenderEvidence {
            manifest: &manifest,
            episode,
            document: doc,
            frames: &frames,
            all_cues,
            recipe: &recipe,
        },
    )
}
