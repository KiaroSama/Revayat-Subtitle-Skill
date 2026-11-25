use anyhow::{Result, anyhow};
use revayat_cli::install::{distribution, transaction};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const VERSION: &str = "3.0.0";
const SKILL: &str = "skills/revayat-subtitle";
const WORKFLOW: &str = "skills/revayat-subtitle/references/multi-agent-workflow.md";
const PLUGINS: &[&str] = &[
    "plugin.json",
    ".antigravity-plugin/plugin.json",
    ".claude-plugin/plugin.json",
    ".codex-plugin/plugin.json",
    ".cursor-plugin/plugin.json",
];
const CATALOGS: &[&str] = &[
    ".agents/plugins/marketplace.json",
    ".claude-plugin/marketplace.json",
    ".cursor-plugin/marketplace.json",
];
const STAGES: &[&str] = &[
    "Initial Translation",
    "Typesetting & Layout",
    "Editorial Review",
    "Translation Applier & Edition Builder",
    "File Re-Check & Issue Fixer",
    "Final Review & Delivery",
];
const MATRIX: &[(&str, &str)] = &[
    ("ubuntu-24.04", "x86_64-unknown-linux-gnu"),
    ("ubuntu-24.04-arm", "aarch64-unknown-linux-gnu"),
    ("windows-2025", "x86_64-pc-windows-msvc"),
    ("windows-11-arm", "aarch64-pc-windows-msvc"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .into()
}
fn text(path: &str) -> Result<String> {
    Ok(fs::read_to_string(root().join(path))?)
}
fn manifest(relative: &str) -> Result<Value> {
    Ok(serde_json::from_str(&text(relative)?)?)
}
fn workflow(name: &str) -> Result<String> {
    text(&format!(".github/workflows/{name}.yml"))
}

fn release_metadata(data: &Value) {
    assert_eq!(data["name"], "revayat-subtitle");
    assert_eq!(data["version"], VERSION);
    assert_eq!(data["license"], "GPL-3.0-or-later");
    assert!(
        data["repository"]
            .as_str()
            .is_some_and(|s| s.ends_with("/Revayat-Subtitle-Skill"))
    );
    assert!(data["description"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(
        data["author"]["name"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(
        data["author"]["email"]
            .as_str()
            .is_some_and(|s| s.contains('@'))
    );
}

#[test]
fn declared_schema_urls_follow_host_contracts() -> Result<()> {
    let schemas = [
        (
            "plugin.json",
            "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
        ),
        (
            ".antigravity-plugin/plugin.json",
            "https://antigravity.google/schemas/v1/plugin.json",
        ),
        (
            ".claude-plugin/marketplace.json",
            "https://code.claude.com/schemas/marketplace.json",
        ),
    ];
    for path in PLUGINS.iter().chain(CATALOGS) {
        let expected = schemas
            .iter()
            .find_map(|(candidate, schema)| (candidate == path).then_some(*schema));
        assert_eq!(manifest(path)?["$schema"].as_str(), expected, "{path}");
    }
    Ok(())
}

macro_rules! manifests {
    ($($name:ident => $path:literal),+ $(,)?) => {$(
        #[test] fn $name() -> Result<()> { release_metadata(&manifest($path)?); Ok(()) }
    )+};
}
manifests! {
    portable_plugin_version => "plugin.json", antigravity_plugin_version => ".antigravity-plugin/plugin.json",
    claude_plugin_version => ".claude-plugin/plugin.json", codex_plugin_version => ".codex-plugin/plugin.json",
    cursor_plugin_version => ".cursor-plugin/plugin.json",
}

fn catalog(path: &str) -> Result<Value> {
    let data = manifest(path)?;
    let entries = data["plugins"].as_array().expect("marketplace entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["version"], VERSION);
    assert_eq!(entries[0]["name"], "revayat-subtitle");
    Ok(entries[0].clone())
}

#[test]
fn claude_marketplace_points_to_local_release() -> Result<()> {
    let item = catalog(".claude-plugin/marketplace.json")?;
    release_metadata(&item);
    assert_eq!(item["source"], "./");
    Ok(())
}

#[test]
fn cursor_marketplace_points_to_local_release() -> Result<()> {
    let item = catalog(".cursor-plugin/marketplace.json")?;
    release_metadata(&item);
    assert_eq!(item["source"], "./");
    Ok(())
}

#[test]
fn agents_marketplace_points_to_git_release() -> Result<()> {
    let item = catalog(".agents/plugins/marketplace.json")?;
    assert_eq!(item["source"]["source"], "url");
    assert!(item["source"]["url"].as_str().is_some_and(
        |s| s.starts_with("https://github.com/") && s.ends_with("/Revayat-Subtitle-Skill.git")
    ));
    assert_eq!(item["policy"]["installation"], "AVAILABLE");
    assert_eq!(item["policy"]["authentication"], "NONE");
    Ok(())
}

fn registered(value: &Value) -> Vec<&str> {
    match value {
        Value::Null => Vec::new(),
        Value::String(path) => vec![path],
        Value::Array(list) => list
            .iter()
            .map(|v| v.as_str().expect("registered path"))
            .collect(),
        _ => panic!("invalid registration: {value}"),
    }
}

#[test]
fn every_registered_component_is_inside_checkout() -> Result<()> {
    let checkout = fs::canonicalize(root())?;
    for plugin in PLUGINS {
        let data = manifest(plugin)?;
        for path in registered(&data["skills"])
            .into_iter()
            .chain(registered(&data["commands"]))
        {
            let relative = path
                .strip_prefix("./")
                .ok_or_else(|| anyhow!("missing relative prefix: {path}"))?;
            assert!(
                !relative.contains(['\\', ':']) && !relative.split('/').any(|part| part == "..")
            );
            assert!(fs::canonicalize(checkout.join(relative))?.starts_with(&checkout));
        }
    }
    Ok(())
}

#[test]
fn registered_skill_directories_contain_skill_documents() -> Result<()> {
    for plugin in PLUGINS {
        let data = manifest(plugin)?;
        for path in registered(&data["skills"]) {
            let base = root().join(path);
            if path == "./skills/" {
                for skill in ["revayat-subtitle", "revayat-subtitle-sync"] {
                    assert!(base.join(skill).join("SKILL.md").is_file());
                }
            } else {
                assert!(base.join("SKILL.md").is_file(), "{plugin}: {path}");
            }
        }
    }
    Ok(())
}

#[test]
fn only_claude_and_cursor_register_slash_commands() -> Result<()> {
    let expected = [
        "./commands/translate-subtitles.md",
        "./commands/revayat-subtitle-qa.md",
        "./commands/revayat-subtitle-resume.md",
        "./commands/revayat-subtitle-sync.md",
    ];
    for plugin in PLUGINS {
        let data = manifest(plugin)?;
        let commands = registered(&data["commands"]);
        if [".claude-plugin/plugin.json", ".cursor-plugin/plugin.json"].contains(plugin) {
            assert_eq!(commands, expected);
        } else {
            assert!(commands.is_empty(), "{plugin}");
        }
        for relative in commands {
            assert!(root().join(relative).is_file());
        }
    }
    Ok(())
}

fn install_bundle(plugin: bool) -> Result<(tempfile::TempDir, PathBuf)> {
    let temp = tempfile::tempdir()?;
    let target = temp.path().join("project/skills/revayat-subtitle");
    let plan = transaction::plan_install(
        &root(),
        std::slice::from_ref(&target),
        plugin,
        false,
        Some(&temp.path().join("recovery")),
        !plugin,
    )?;
    transaction::execute_plan(plan, temp.path())?;
    Ok((temp, target))
}

#[test]
fn standard_install_includes_both_skills_and_bundled_voice_and_ffmpeg_assets() -> Result<()> {
    let (_temp, target) = install_bundle(false)?;
    for asset in [
        "SKILL.md",
        "voice/humanizer/SKILL.md",
        "voice/persian/SKILL.md",
        "voice/great-russian/SKILL.md",
        "ffmpeg/SKILL.md",
        "ffmpeg/references/sync-and-probing.md",
    ] {
        assert_eq!(
            fs::read(target.join(asset))?,
            fs::read(root().join(SKILL).join(asset))?,
            "{asset}"
        );
    }
    assert!(
        target
            .with_file_name("revayat-subtitle-sync")
            .join("SKILL.md")
            .is_file()
    );
    Ok(())
}

#[test]
fn plugin_install_includes_all_manifests_commands_and_nested_skills() -> Result<()> {
    let (_temp, target) = install_bundle(true)?;
    for path in PLUGINS.iter().chain(CATALOGS) {
        assert_eq!(fs::read(target.join(path))?, fs::read(root().join(path))?);
    }
    for path in [
        "commands/translate-subtitles.md",
        "commands/revayat-subtitle-qa.md",
        "commands/revayat-subtitle-resume.md",
        "commands/revayat-subtitle-sync.md",
        "skills/revayat-subtitle/SKILL.md",
        "skills/revayat-subtitle-sync/SKILL.md",
        "skills/revayat-subtitle/ffmpeg/SKILL.md",
        "skills/revayat-subtitle/voice/persian/SKILL.md",
    ] {
        assert_eq!(fs::read(target.join(path))?, fs::read(root().join(path))?);
    }
    Ok(())
}

#[test]
fn distribution_manifest_does_not_ship_language_wrappers() -> Result<()> {
    let payloads = distribution::payloads(&root(), true, true)?;
    let manifest = distribution::distribution_manifest(&payloads["revayat-subtitle"])?;
    let files = manifest["files"].as_object().expect("distribution files");
    assert!(files.contains_key("skills/revayat-subtitle/voice/humanizer/SKILL.md"));
    assert!(!files.keys().any(|path| script(Path::new(path))));
    Ok(())
}

fn script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["py", "ps1", "sh"]
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
}

fn no_scripts(path: &str) -> Result<()> {
    for entry in walkdir::WalkDir::new(root().join(path)) {
        let entry = entry?;
        assert!(
            !entry.file_type().is_file() || !script(entry.path()),
            "legacy script: {}",
            entry.path().display()
        );
    }
    Ok(())
}

#[test]
fn crates_have_no_python_or_shell_scripts() -> Result<()> {
    no_scripts("crates")
}
#[test]
fn imported_ffmpeg_skill_has_no_scripts() -> Result<()> {
    no_scripts("skills/revayat-subtitle/ffmpeg")
}

fn stages(path: &str) -> Result<()> {
    let data = text(path)?.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut rest = data.as_str();
    for stage in STAGES {
        rest = rest
            .split_once(stage)
            .unwrap_or_else(|| panic!("missing or misordered {stage} in {path}"))
            .1;
    }
    Ok(())
}

#[test]
fn skill_lists_stages_in_order() -> Result<()> {
    stages("skills/revayat-subtitle/SKILL.md")
}
#[test]
fn workflow_reference_lists_stages_in_order() -> Result<()> {
    stages("skills/revayat-subtitle/references/workflow.md")
}
#[test]
fn translation_command_lists_stages_in_order() -> Result<()> {
    stages("commands/translate-subtitles.md")
}
#[test]
fn agent_reference_lists_stages_in_order() -> Result<()> {
    stages(WORKFLOW)
}

#[test]
fn every_stage_has_a_copy_ready_role_prompt() -> Result<()> {
    let workflow = text(WORKFLOW)?;
    let body = workflow
        .split_once("## Copy-ready prompts")
        .expect("copyable prompts")
        .1;
    let sections: Vec<_> = body.split("### Stage ").skip(1).collect();
    assert_eq!(sections.len(), STAGES.len());
    for (i, (section, stage)) in sections.iter().zip(STAGES).enumerate() {
        assert!(section.starts_with(&format!("{}: {stage} Agent", i + 1)));
        let prompt = section
            .split_once("```text")
            .and_then(|(_, body)| body.split_once("```"))
            .expect("fenced role prompt")
            .0;
        assert!(
            prompt.contains(&format!("You are the {stage} Agent")) && prompt.contains("{{EPISODE")
        );
        assert!(
            prompt.contains("{{ACTIVITY_LOG}}"),
            "missing activity log in stage {}",
            i + 1
        );
    }
    Ok(())
}

#[test]
fn rtl_prompt_keeps_last_punctuation_inside_the_isolate() -> Result<()> {
    let workflow = text(WORKFLOW)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(workflow.contains("RLI (U+2067) and close it with PDI (U+2069) after the last character, including any terminal punctuation"));
    Ok(())
}

#[test]
fn review_prompts_require_bold_italic_preservation_and_no_untranslated_cues() -> Result<()> {
    let data = text(WORKFLOW)?;
    for section in data
        .split("### Stage ")
        .filter(|s| s.starts_with(['3', '6']))
    {
        assert!(section.contains("zero untranslated cues"));
        assert!(section.contains("bold/italic"));
    }
    assert_eq!(data.matches("zero untranslated cues").count(), 3);
    Ok(())
}

#[test]
fn workflow_keeps_bundled_b_koodak_font() -> Result<()> {
    assert!(text(WORKFLOW)?.contains("B Koodak (assets/fonts/BKOODB.TTF)"));
    assert!(root().join(SKILL).join("assets/fonts/BKOODB.TTF").is_file());
    Ok(())
}

#[test]
fn editorial_review_requires_canon_source_checks_before_delivery() -> Result<()> {
    let docs = text(WORKFLOW)?;
    assert!(docs.contains("online canon sources from stage 3"));
    assert!(docs.contains("proper nouns, titles, franchise lore"));
    for stage in [1, 3] {
        let section = docs
            .split(&format!("### Stage {stage}:"))
            .nth(1)
            .expect("agent stage");
        assert!(
            section
                .split("### Stage ")
                .next()
                .expect("stage body")
                .contains("revayat-subtitle lookup --json"),
            "missing terminology lookup in stage {stage}"
        );
    }
    Ok(())
}

#[test]
fn voice_reference_and_bundled_humanizer_agree_at_v4() -> Result<()> {
    assert!(text(WORKFLOW)?.contains("voice/humanizer/SKILL.md (v4.0.0)"));
    assert!(
        text("skills/revayat-subtitle/voice/humanizer/SKILL.md")?.contains("version: \"4.0.0\"")
    );
    Ok(())
}

#[test]
fn main_skill_links_to_the_imported_ffmpeg_instructions() -> Result<()> {
    assert!(text("skills/revayat-subtitle/SKILL.md")?.contains("(ffmpeg/SKILL.md)"));
    let imported = text("skills/revayat-subtitle/ffmpeg/SKILL.md")?;
    assert!(
        imported.starts_with("---\nname: ffmpeg\n")
            || imported.starts_with("---\r\nname: ffmpeg\r\n")
    );
    assert!(
        imported.contains("revayat-subtitle sync")
            && imported.contains("references/sync-and-probing.md")
    );
    Ok(())
}

fn field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let line = line.trim().trim_start_matches("- ");
    let value = line.strip_prefix(field)?.strip_prefix(':')?;
    Some(value.split(" #").next()?.trim().trim_matches(['\'', '"']))
}

fn supported_matrix(path: &str) -> Result<()> {
    let data = workflow(path)?;
    let mut seen = Vec::new();
    let mut runner = None;
    for line in data.lines() {
        if let Some(found) = field(line, "runner") {
            runner = Some(found);
        }
        if let Some(target) = field(line, "target") {
            seen.push((runner.take().expect("runner before target"), target));
        }
        if let Some(run) = field(line, "runs-on") {
            assert!(
                run == "${{ matrix.runner }}" || MATRIX.iter().any(|(runner, _)| *runner == run),
                "unsupported runner {run} in {path}"
            );
        }
    }
    assert_eq!(seen.len(), MATRIX.len());
    assert_eq!(
        seen.into_iter().collect::<BTreeSet<_>>(),
        MATRIX.iter().copied().collect()
    );
    let lower = data.to_ascii_lowercase();
    for forbidden in [
        "macos",
        "apple-darwin",
        "i686",
        "i586",
        "armv7",
        "win-x86",
        "win32",
        "setup-python",
        "pytest",
        "python ",
        "python3 ",
    ] {
        assert!(!lower.contains(forbidden), "{path} contains {forbidden}");
    }
    Ok(())
}

#[test]
fn ci_only_builds_the_four_supported_rust_targets() -> Result<()> {
    supported_matrix("ci")
}
#[test]
fn release_only_builds_the_four_supported_rust_targets() -> Result<()> {
    supported_matrix("release")
}

fn pinned(name: &str) -> Result<()> {
    let data = workflow(name)?;
    let actions: Vec<_> = data
        .lines()
        .filter_map(|line| field(line, "uses"))
        .collect();
    assert!(!actions.is_empty(), "{name} has no action references");
    for action in actions {
        if action.starts_with("./") {
            assert!(root().join(action).is_file());
            continue;
        }
        let (_, pin) = action
            .rsplit_once('@')
            .unwrap_or_else(|| panic!("missing SHA for {action}"));
        assert!(
            pin.len() == 40 && pin.bytes().all(|b| b.is_ascii_hexdigit()),
            "unpinned action: {action}"
        );
    }
    Ok(())
}

#[test]
fn ci_actions_are_pinned() -> Result<()> {
    pinned("ci")
}
#[test]
fn release_actions_are_pinned() -> Result<()> {
    pinned("release")
}
#[test]
fn codeql_actions_are_pinned() -> Result<()> {
    pinned("codeql")
}

#[test]
fn ci_checks_main_pushes_prs_merge_groups_and_manual_runs_without_path_filters() -> Result<()> {
    let ci = workflow("ci")?;
    let triggers = ci
        .split_once("\non:")
        .expect("event triggers")
        .1
        .split_once("\npermissions:")
        .expect("permissions")
        .0;
    for event in [
        "push",
        "pull_request",
        "merge_group",
        "workflow_dispatch",
        "workflow_call",
    ] {
        assert!(
            triggers.lines().any(|line| line == format!("  {event}:")),
            "missing {event}"
        );
    }
    assert!(triggers.contains("branches: [main]"));
    for forbidden in ["paths:", "paths-ignore:", "pull_request_target:"] {
        assert!(!triggers.contains(forbidden));
    }
    assert!(ci.contains("uses: ./.github/workflows/workflow-lint.yml"));
    assert!(ci.contains("persist-credentials: false"));
    Ok(())
}

#[test]
fn workflow_lint_cannot_run_with_pr_target_privileges() -> Result<()> {
    let data = workflow("workflow-lint")?;
    assert!(data.contains("workflow_call:"));
    assert!(data.contains("persist-credentials: false"));
    assert!(!data.contains("pull_request_target:"));
    pinned("workflow-lint")
}

#[test]
fn release_validation_is_scoped_to_its_run() -> Result<()> {
    assert!(workflow("ci")?.contains("inputs.validation_id || ''"));
    let release = workflow("release")?;
    for token in [
        "validation_id: ${{ github.run_id }}",
        "needs: validate",
        "cancel-in-progress: false",
    ] {
        assert!(release.contains(token));
    }
    Ok(())
}

#[test]
fn codeql_covers_rust_and_actions_and_shares_a_dependabot_pin_group() -> Result<()> {
    let codeql = workflow("codeql")?;
    assert!(
        codeql.contains("language: [rust, actions]")
            && codeql.contains("languages: ${{ matrix.language }}")
    );
    let pin = |action| {
        codeql
            .lines()
            .filter_map(|line| field(line, "uses"))
            .find_map(|line| line.strip_prefix(action))
    };
    let init = pin("github/codeql-action/init@").expect("init pin");
    assert_eq!(pin("github/codeql-action/analyze@"), Some(init));
    assert!(text(".github/dependabot.yml")?.contains("'github/codeql-action/*'"));
    Ok(())
}

fn badges(path: &str) -> Result<()> {
    let contents = text(path)?;
    let urls: Vec<_> = contents
        .lines()
        .filter_map(|line| line.split_once("(https://img.shields.io/").map(|(_, s)| s))
        .collect();
    assert!(
        (12..=16).contains(&urls.len()),
        "{path}: {} badges",
        urls.len()
    );
    let mut colors = BTreeSet::new();
    for badge in &urls {
        let (url, link) = badge.split_once(")](").expect("badge destination");
        assert!(!link.trim_end_matches(')').is_empty());
        // The donation badge has a fixed URL in the publishing policy.
        assert!(url.contains("style=flat-square") || url == "badge/Support-donations-d04a9a");
        let color = url
            .split_once("color=")
            .map(|(_, s)| s.split('&').next().unwrap_or(s))
            .or_else(|| url.split('?').next()?.rsplit_once('-').map(|(_, c)| c))
            .expect("hex color");
        assert!(color.len() == 6 && color.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(
            colors.insert(color.to_ascii_lowercase()),
            "duplicate badge color {color}"
        );
    }
    assert!(urls.iter().any(|url| url.contains("/Rust-1.100.0-8b5cf6")));
    assert!(
        urls.iter()
            .any(|url| url.contains("/Platform-") && url.contains("0891b2"))
    );
    assert!(
        urls.iter()
            .any(|url| url.contains("repo-size/") && url.contains("ca8a04"))
    );
    assert_eq!(
        urls.last(),
        Some(&"badge/Support-donations-d04a9a)](#donate)")
    );
    Ok(())
}

#[test]
fn english_readme_badges_are_distinct_and_synchronized() -> Result<()> {
    badges("README.md")
}
#[test]
fn persian_readme_badges_are_distinct_and_synchronized() -> Result<()> {
    badges("README.fa.md")
}

fn readme_tail(path: &str) -> Result<()> {
    let data = text(path)?;
    let headings: Vec<_> = data
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();
    assert!(headings.ends_with(&["## Author", "## License", "## Donate"]));
    Ok(())
}

#[test]
fn english_readme_keeps_required_ending_sections() -> Result<()> {
    readme_tail("README.md")
}
#[test]
fn persian_readme_keeps_required_ending_sections() -> Result<()> {
    readme_tail("README.fa.md")
}

#[test]
fn generated_build_and_private_state_paths_are_ignored() -> Result<()> {
    let ignore = text(".gitignore")?;
    let entries: BTreeSet<_> = ignore.lines().map(str::trim).collect();
    for required in [
        "/.ai/",
        "/.specify/",
        "/specs/",
        "/graphify-out/",
        "/bin/",
        "/target/",
        "/secrets.md",
    ] {
        assert!(entries.contains(required), "missing ignore rule {required}");
    }
    Ok(())
}
