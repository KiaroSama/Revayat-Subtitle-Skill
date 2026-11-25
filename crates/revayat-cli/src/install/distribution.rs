use super::files::{
    MAX_FILE, fingerprint, invalid, linked, manifest_name, metadata, safe_path, validate_manifest,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};

pub const NAME: &str = "revayat-subtitle";
pub const SYNC: &str = "revayat-subtitle-sync";
pub const AGENTS: &[&str] = &[
    "claude",
    "codex",
    "cursor",
    "kiro",
    "cline",
    "hermes",
    "opencode",
    "antigravity",
    "antigravity-cli",
];
const MANIFESTS: &[&str] = &[
    "plugin.json",
    ".codex-plugin/plugin.json",
    ".claude-plugin/plugin.json",
    ".claude-plugin/marketplace.json",
    ".cursor-plugin/plugin.json",
    ".cursor-plugin/marketplace.json",
    ".agents/plugins/marketplace.json",
    ".antigravity-plugin/plugin.json",
];
const BINARIES: &[&str] = &[NAME, "subsync"];
pub type Payload = Vec<(PathBuf, PathBuf)>;

pub fn destination(agent: &str, scope: &str, base: &Path) -> Option<PathBuf> {
    if !matches!(scope, "user" | "project") {
        return None;
    }
    let folder = match (agent, scope == "user") {
        ("claude", _) => ".claude",
        ("cursor", _) => ".cursor",
        ("kiro", _) => ".kiro",
        ("cline", _) => ".cline",
        ("hermes", _) => ".hermes",
        ("opencode", true) => ".config/opencode",
        ("opencode", false) => ".opencode",
        ("antigravity", true) => ".gemini/antigravity",
        ("antigravity-cli", true) => ".gemini/antigravity-cli",
        ("codex" | "antigravity" | "antigravity-cli", _) => ".agents",
        _ => return None,
    };
    Some(base.join(folder).join("skills").join(NAME))
}

pub fn protected(root: &Path) -> Vec<PathBuf> {
    [
        "skills",
        "commands",
        "bin",
        "LICENSE",
        ".codex-plugin",
        ".claude-plugin",
        ".cursor-plugin",
        ".antigravity-plugin",
        ".agents/plugins",
        "plugin.json",
    ]
    .map(|path| root.join(path))
    .to_vec()
}

pub fn distribution_files(files: &[PathBuf]) -> io::Result<()> {
    for file in files {
        if !safe_path(file, false)?.is_file() {
            return Err(invalid("Distribution contains a missing file"));
        }
    }
    Ok(())
}

fn discover(folder: &Path, extensions: &[&str]) -> io::Result<Vec<PathBuf>> {
    let folder = safe_path(folder, true)?;
    let mut files = Vec::new();
    for (i, entry) in walkdir::WalkDir::new(folder)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .enumerate()
    {
        if i >= 10_000 {
            return Err(invalid("Distribution exceeds its entry limit"));
        }
        let entry = entry.map_err(io::Error::other)?;
        let meta = entry.path().symlink_metadata()?;
        if linked(&meta) || !(meta.is_dir() || meta.is_file()) {
            return Err(invalid(
                "Distribution contains a link or unsupported file type",
            ));
        }
        let extension = entry.path().extension().and_then(|s| s.to_str());
        let legal = entry
            .file_name()
            .to_str()
            .is_some_and(|n| matches!(n, "LICENSE" | "NOTICE"));
        if meta.is_file()
            && (legal
                || extension.is_some_and(|ext| {
                    extensions
                        .iter()
                        .any(|allowed| ext.eq_ignore_ascii_case(allowed))
                }))
        {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

pub fn skill_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let skill = root.join("skills").join(NAME);
    distribution_files(&[skill.join("SKILL.md")])?;
    discover(
        &skill,
        &[
            "md", "yaml", "yml", "tsv", "json", "ttf", "otf", "txt", "jxl", "png", "svg",
        ],
    )
}

fn place(files: Vec<PathBuf>, source_root: &Path, prefix: &Path) -> io::Result<Payload> {
    files
        .into_iter()
        .map(|source| {
            let relative = prefix.join(source.strip_prefix(source_root).map_err(io::Error::other)?);
            Ok((source, relative))
        })
        .collect()
}

pub fn payloads(
    root: &Path,
    plugin: bool,
    include_sync: bool,
) -> io::Result<BTreeMap<String, Payload>> {
    let root = safe_path(root, true)?;
    let skill = root.join("skills").join(NAME);
    let sync = root.join("skills").join(SYNC);
    let mut main = place(
        skill_files(&root)?,
        &skill,
        if plugin {
            Path::new("skills/revayat-subtitle")
        } else {
            Path::new("")
        },
    )?;
    let mut result = BTreeMap::new();
    if plugin || include_sync {
        distribution_files(&[sync.join("SKILL.md")])?;
        let sync_files = discover(&sync, &["md", "yaml", "yml", "txt"])?;
        if plugin {
            main.extend(place(
                sync_files,
                &sync,
                Path::new("skills/revayat-subtitle-sync"),
            )?);
        } else {
            result.insert(SYNC.into(), place(sync_files, &sync, Path::new(""))?);
        }
    }
    if plugin {
        let files: Vec<_> = MANIFESTS
            .iter()
            .chain([&"LICENSE"])
            .map(|p| root.join(p))
            .collect();
        distribution_files(&files)?;
        main.extend(place(files, &root, Path::new(""))?);
        let commands = discover(&root.join("commands"), &["md"])?;
        if commands.is_empty() {
            return Err(invalid("Plugin distribution has no commands"));
        }
        main.extend(place(commands, &root, Path::new(""))?);
    }
    for name in BINARIES {
        let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let relative = PathBuf::from("bin").join(&name);
        let binary = root.join(&relative);
        if let Some(meta) = metadata(&binary)? {
            if meta.len() > MAX_FILE {
                return Err(invalid("Distribution binary exceeds its byte limit"));
            }
            distribution_files(std::slice::from_ref(&binary))?;
            main.push((binary, relative));
        }
    }
    result.insert(NAME.into(), main);
    Ok(result)
}

pub(super) fn set_binary_permissions(file: &std::fs::File, relative: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if relative.parent() == Some(Path::new("bin"))
            && relative
                .file_name()
                .and_then(|leaf| leaf.to_str())
                .is_some_and(|leaf| BINARIES.contains(&leaf))
        {
            file.set_permissions(std::fs::Permissions::from_mode(0o755))?;
        }
    }
    #[cfg(not(unix))]
    let _ = (file, relative);
    Ok(())
}

pub fn distribution_manifest(files: &[(PathBuf, PathBuf)]) -> io::Result<Value> {
    let mut expected = BTreeMap::new();
    let mut directories = BTreeSet::new();
    for (source, relative) in files {
        let name = relative
            .to_str()
            .ok_or_else(|| invalid("Distribution path is not UTF-8"))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        if !manifest_name(&name) || expected.insert(name, fingerprint(source)?).is_some() {
            return Err(invalid("Invalid or duplicate distribution relative path"));
        }
        for parent in relative
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
        {
            directories.insert(
                parent
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
    }
    let manifest = json!({"files": expected, "directories": directories});
    validate_manifest(&manifest)?;
    Ok(manifest)
}
