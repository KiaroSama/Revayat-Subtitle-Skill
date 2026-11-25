//! FFmpeg/libass render evidence and hash-checked subtitle delivery.

#![forbid(unsafe_code)]

pub mod delivery;
pub mod renderer;
pub mod sampling;

use crate::subtitle_formats::document::{Cue, Document};
use crate::workflow::contract::{
    attr_path, decode, item, items, iterable, marked, path_marker, string, value_error,
    workspace_path,
};
use crate::workflow::workspace::utf8_path;
use renderer::RenderEvidence;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

pub fn bundled_fonts_dir() -> Result<Option<Value>, String> {
    Ok(renderer::bundled_fonts_dir().as_deref().map(path_marker))
}

pub fn ffmpeg_path(r#override: Option<String>) -> Result<String, String> {
    Ok(utf8_path(&renderer::ffmpeg_path(r#override.as_deref())?)?.into())
}

pub fn subtitle_snapshot(build: Value, episode: Value) -> Result<Vec<u8>, String> {
    item(&episode, "file")?;
    delivery::subtitle_snapshot(&workspace_path(&build)?, &episode)
}

pub fn doctor(r#override: Option<String>) -> Result<Value, String> {
    renderer::doctor(r#override.as_deref())
}

pub fn load_build(build: Value) -> Result<Value, String> {
    let (manifest, documents) = delivery::load_build(&workspace_path(&build)?)?;
    let mut ordered = Map::new();
    for episode in items(&manifest["episodes"]) {
        let id = string(&episode["id"]);
        ordered.insert(id.into(), marked("Document", &documents[id])?);
    }
    Ok(json!({"$tuple": [manifest, ordered]}))
}

pub fn animated_event(cue: Value, commands: Value) -> Result<bool, String> {
    let cue: Cue = decode("Cue", cue, "fields")?;
    if sampling::animated(&cue, &[]) {
        return Ok(true);
    }
    let commands = iterable(&commands)?
        .iter()
        .map(|v| {
            let pair = iterable(v)?;
            let [name, argument] = pair else {
                return Err(value_error("Invalid animation command"));
            };
            Ok((string(name).into(), string(argument).into()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(sampling::animated(&cue, &commands))
}

pub fn sample_plan(
    doc: Value,
    all_cues: bool,
    changed_indices: Option<Value>,
) -> Result<Vec<Value>, String> {
    let doc: Document = decode("Document", doc, "cues")?;
    if doc.cues.is_empty() {
        return Err(value_error("Render sampling requires at least one cue"));
    }
    let changed = changed_indices
        .as_ref()
        .map(iterable)
        .transpose()?
        .unwrap_or_default()
        .iter()
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| "TypeError: changed cue index must be an integer".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    sampling::samples(&doc, all_cues, &changed)
}

pub fn sample_times(doc: Value, all_cues: bool) -> Result<Vec<f64>, String> {
    sample_plan(doc, all_cues, None)?
        .iter()
        .map(|v| {
            v["time_seconds"]
                .as_f64()
                .ok_or_else(|| "TypeError: invalid sample time".to_owned())
        })
        .collect()
}

pub fn changed_indices(episode: Value) -> Result<Vec<i64>, String> {
    sampling::changed_indices(&episode)
}

pub fn requested_fonts(doc: Value) -> Result<Vec<String>, String> {
    let doc: Document = decode("Document", doc, "style_fields")?;
    sampling::requested_fonts(&doc)
}

pub fn write_render_evidence(
    build: Value,
    manifest: Value,
    episode: Value,
    doc: Value,
    frames: Vec<Value>,
    all_cues: bool,
    recipe: Value,
) -> Result<Value, String> {
    item(&episode, "id")?;
    let build = workspace_path(&build)?;
    let doc: Document = decode("Document", doc, "cues")?;
    renderer::write_evidence(
        &build,
        &RenderEvidence {
            manifest: &manifest,
            episode: &episode,
            document: &doc,
            frames: &frames,
            all_cues,
            recipe: &recipe,
        },
    )
}

pub fn render(
    build: Value,
    episode_id: String,
    r#override: Option<String>,
    video: Option<Value>,
    fonts: Option<Value>,
    all_cues: bool,
) -> Result<Value, String> {
    let build = workspace_path(&build)?;
    let video = video.as_ref().map(|v| attr_path(v, "lstat")).transpose()?;
    let fonts = fonts.as_ref().map(|v| attr_path(v, "is_dir")).transpose()?;
    renderer::render(
        &build,
        &episode_id,
        r#override.as_deref(),
        video.as_deref(),
        fonts.as_deref(),
        all_cues,
    )
}

pub fn check_delivery(
    build: Value,
    require_review: bool,
    episode_ids: Option<Value>,
) -> Result<Value, String> {
    let build = workspace_path(&build)?;
    let ids = episode_ids
        .as_ref()
        .map(|v| iterable(v).map(|ids| ids.iter().map(string).collect::<HashSet<_>>()))
        .transpose()?;
    let delivery::Delivery {
        manifest,
        names,
        backgrounds,
        frame_count,
    } = delivery::check_delivery(&build, require_review, ids.as_ref())?;
    Ok(json!({"$tuple": [manifest, names, {"$set": backgrounds}, frame_count]}))
}

pub fn qa(build: Value) -> Result<Value, String> {
    delivery::qa(&workspace_path(&build)?)
}

pub fn package(build: Value, output: Value) -> Result<Value, String> {
    let build = workspace_path(&build)?;
    delivery::package(&build, &attr_path(&output, "exists")?)
}
