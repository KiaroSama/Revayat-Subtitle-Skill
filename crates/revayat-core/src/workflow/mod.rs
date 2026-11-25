//! Workspace inventory, editorial review, and immutable subtitle editions.

#![forbid(unsafe_code)]

pub mod contract;
pub mod contract_json;
pub mod import;
pub mod review;
#[cfg(test)]
mod tests;
pub mod workspace;
pub use crate::subtitle_formats::document::PresentationOrder;
pub use import::ImportOptions;
pub use review::CueDecision;
pub use workspace::{RECIPE_VERSION, RecipeVersion};

use crate::subtitle_formats::document::{Cue, Document};
use crate::validation;
use contract::{attr_path, decode, marked, path_marker, truth, value_error, workspace_path};
use serde_json::{Map, Value, json};

pub fn input_candidates(path: Value, excluded_root: Option<Value>) -> Result<Value, String> {
    let excluded = excluded_root
        .as_ref()
        .map(|value| attr_path(value, "resolve"))
        .transpose()?;
    let path = attr_path(
        &path,
        if excluded.is_some() {
            "resolve"
        } else {
            "is_dir"
        },
    )?;
    Ok(json!(
        import::input_candidates(&path, excluded.as_deref())?
            .iter()
            .map(|p| path_marker(p))
            .collect::<Result<Vec<_>, _>>()?
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserves the recorded prepare call; typed callers use ImportOptions"
)]
pub fn prepare(
    paths: Vec<Value>,
    work: Value,
    series: String,
    season: i64,
    encoding: String,
    glossary: Option<Value>,
    target_language: String,
    output_format: String,
) -> Result<Value, String> {
    workspace::format(&output_format)?;
    validation::string(json!(series), "series".into(), false)?;
    validation::integer(json!(season), "season".into(), 1, Some(99))?;
    validation::text_encoding(json!(encoding), "encoding".into())?;
    if series.trim().is_empty() {
        return Err(value_error(
            "A series title and season from 1 to 99 are required",
        ));
    }
    let work = attr_path(&work, "exists")?;
    let glossary = glossary
        .as_ref()
        .filter(|v| truth(v))
        .map(|value| attr_path(value, "lstat"))
        .transpose()?;
    let paths = paths
        .iter()
        .map(|value| attr_path(value, "is_symlink"))
        .collect::<Result<Vec<_>, _>>()?;
    import::prepare(
        &paths,
        &work,
        &import::ImportOptions {
            series: &series,
            season,
            encoding: &encoding,
            glossary: glossary.as_deref(),
            language: &target_language,
            output_format: &output_format,
        },
    )
}

pub fn docs_value(docs: workspace::Documents, project: Option<&Value>) -> Result<Value, String> {
    let mut out = Map::new();
    if let Some(project) = project {
        for source in contract::items(&project["sources"]) {
            let key = contract::string(&source["id"]);
            if let Some(doc) = docs.get(key) {
                out.insert(key.into(), marked("Document", doc)?);
            }
        }
    } else {
        for (key, doc) in docs {
            out.insert(key, marked("Document", &doc)?);
        }
    }
    Ok(Value::Object(out))
}

pub fn load(work: Value) -> Result<Value, String> {
    let workspace::Workspace {
        project,
        documents,
        sheets,
    } = workspace::load(&workspace_path(&work)?)?;
    let documents = docs_value(documents, Some(&project))?;
    let mut ordered = Map::new();
    for key in documents.as_object().into_iter().flat_map(Map::keys) {
        ordered.insert(key.clone(), json!(sheets[key]));
    }
    Ok(json!({"$tuple": [project, documents, Value::Object(ordered)]}))
}

pub fn verify_glossary(terms: Value, series: String) -> Result<(), String> {
    review::verify_glossary(terms, &series)
}

pub fn reviewed_cues(
    doc: Value,
    sheet: Vec<Value>,
    target_language: String,
) -> Result<Value, String> {
    let sheet = validation::worksheet(json!(sheet), "worksheet".into())?;
    let doc = decode("Document", doc, "cues")?;
    let (cues, decisions) = review::reviewed_cues(&doc, &sheet, &target_language)?;
    let cues = cues
        .iter()
        .map(|cue| marked("Cue", cue))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"$tuple": [cues, decisions]}))
}

pub fn merge_donor(
    base: &mut Value,
    donor: Value,
    cues: Vec<Value>,
    source_id: String,
    keep_fonts: bool,
) -> Result<Vec<Value>, String> {
    if cues.is_empty() {
        return Ok(Vec::new());
    }
    let mut updated: Document = decode("Document", base.clone(), "styles")?;
    let donor: Document = decode("Document", donor, "kind")?;
    let cues = cues
        .into_iter()
        .map(|cue| decode::<Cue>("Cue", cue, "fields"))
        .collect::<Result<Vec<_>, _>>()?;
    let merged = review::donor(&mut updated, &donor, cues, &source_id, keep_fonts)?
        .iter()
        .map(|cue| marked("Cue", cue))
        .collect::<Result<Vec<_>, _>>()?;
    *base = marked("Document", &updated)?;
    Ok(merged)
}

pub fn assemble(work: Value, output_format: Option<String>) -> Result<Value, String> {
    let workspace::Edition {
        manifest,
        files,
        glossary,
    } = workspace::assemble(&workspace_path(&work)?, output_format.as_deref())?;
    let files: Map<_, _> = files
        .into_iter()
        .map(|(key, bytes)| (key, json!({"$bytes": bytes})))
        .collect();
    Ok(json!({"$tuple": [manifest, files, glossary]}))
}

pub fn build(work: Value, output_format: Option<String>) -> Result<Value, String> {
    workspace::build(&workspace_path(&work)?, output_format.as_deref())
}
