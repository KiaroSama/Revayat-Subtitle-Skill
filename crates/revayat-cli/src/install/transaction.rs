use super::{
    distribution::{self, NAME, Payload, SYNC},
    files::*,
    journal::{existing, overlaps, path, path_key},
    recovery, registry,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub type Ancestors = BTreeMap<PathBuf, Option<(u64, u64)>>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub target: PathBuf,
    previous: Option<(u64, u64)>,
    payload: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub targets: Vec<Target>,
    pub recovery: PathBuf,
    pub payloads: BTreeMap<String, Payload>,
    pub ancestors: Ancestors,
}

fn observed(path: &Path) -> io::Result<Option<(u64, u64)>> {
    if existing(path)? {
        Ok(Some(identity_numbers(path)?))
    } else {
        Ok(None)
    }
}

pub fn plan_install(
    root: &Path,
    targets: &[PathBuf],
    plugin: bool,
    force: bool,
    recovery: Option<&Path>,
    include_sync: bool,
) -> io::Result<Plan> {
    let payloads = distribution::payloads(root, plugin, include_sync)?;
    plan_with_payloads(
        root,
        &expand_targets(targets, include_sync && !plugin),
        force,
        recovery,
        payloads,
    )
}

fn expand_targets(targets: &[PathBuf], include_sync: bool) -> Vec<PathBuf> {
    if include_sync {
        targets
            .iter()
            .flat_map(|p| [p.clone(), p.with_file_name(SYNC)])
            .collect()
    } else {
        targets.to_vec()
    }
}

pub fn plan_uninstall(
    root: &Path,
    targets: &[PathBuf],
    plugin: bool,
    recovery: Option<&Path>,
    include_sync: bool,
) -> io::Result<Plan> {
    plan_targets(
        root,
        &expand_targets(targets, include_sync && !plugin),
        true,
        recovery,
        BTreeMap::new(),
    )
}

pub fn plan_with_payloads(
    root: &Path,
    targets: &[PathBuf],
    force: bool,
    recovery: Option<&Path>,
    payloads: BTreeMap<String, Payload>,
) -> io::Result<Plan> {
    if payloads.is_empty() {
        return Err(invalid("Missing installation payload"));
    }
    plan_targets(root, targets, force, recovery, payloads)
}

fn plan_targets(
    root: &Path,
    targets: &[PathBuf],
    force: bool,
    recovery: Option<&Path>,
    payloads: BTreeMap<String, Payload>,
) -> io::Result<Plan> {
    let uninstall = payloads.is_empty();
    let root = safe_path(root, true)?;
    let protected = distribution::protected(&root);
    let mut planned = Vec::<Target>::new();
    let mut ancestors = Ancestors::new();
    if targets.is_empty() || targets.len() > 1000 {
        return Err(invalid(
            "No installation targets selected or target limit exceeded",
        ));
    }
    for original in targets {
        let target = safe_path(original, true)?;
        if path_key(&root).starts_with(path_key(&target))
            || protected.iter().any(|p| overlaps(&target, p))
        {
            return Err(invalid(
                "Installation destination overlaps distribution source files",
            ));
        }
        for parent in target.ancestors().skip(1) {
            ancestors.insert(parent.into(), observed(parent)?);
        }
        if planned
            .iter()
            .any(|old| path_key(&old.target) == path_key(&target))
        {
            continue;
        }
        if planned.iter().any(|old| overlaps(&old.target, &target)) {
            return Err(invalid("Installation targets overlap each other"));
        }
        let previous = observed(&target)?;
        if uninstall && previous.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No installation at {}", target.display()),
            ));
        }
        if previous.is_some() && !force {
            return Err(invalid(
                "An installation already exists; use --force to retain a backup",
            ));
        }
        let payload = if target.file_name().is_some_and(|n| n == SYNC)
            && (uninstall || payloads.contains_key(SYNC))
        {
            SYNC
        } else {
            NAME
        };
        if !uninstall && !payloads.contains_key(payload) {
            return Err(invalid("Missing installation payload"));
        }
        planned.push(Target {
            target,
            previous,
            payload: payload.into(),
        });
    }
    let mut roots = Vec::new();
    for item in &planned {
        let parent = item
            .target
            .parent()
            .ok_or_else(|| invalid("Installation target has no parent"))?;
        let recovery = if let Some(recovery) = recovery {
            recovery.to_path_buf()
        } else {
            if !item
                .target
                .file_name()
                .and_then(|x| x.to_str())
                .is_some_and(|n| matches!(n, NAME | SYNC))
                || parent.file_name().is_none_or(|n| n != "skills")
            {
                return Err(invalid(
                    "Custom installation needs explicit --recovery-dir outside skill/plugin discovery",
                ));
            }
            parent.with_file_name("revayat-recovery")
        };
        let recovery = safe_path(&recovery, true)?;
        if path_key(&root).starts_with(path_key(&recovery))
            || protected.iter().any(|p| overlaps(&recovery, p))
            || planned.iter().any(|p| overlaps(&recovery, &p.target))
            || recovery.iter().any(|p| {
                p.to_str().is_some_and(|name| {
                    #[cfg(windows)]
                    {
                        name.eq_ignore_ascii_case("skills") || name.eq_ignore_ascii_case("plugins")
                    }
                    #[cfg(not(windows))]
                    {
                        matches!(name, "skills" | "plugins")
                    }
                })
            })
        {
            return Err(invalid(
                "Recovery root overlaps source or skill/plugin discovery",
            ));
        }
        if device(&recovery)? != device(parent)? {
            return Err(invalid(
                "Recovery and installation must be on the same filesystem; no copy-delete fallback",
            ));
        }
        for path in recovery.ancestors() {
            ancestors.entry(path.into()).or_insert(observed(path)?);
        }
        roots.push(recovery);
    }
    let recovery = roots
        .into_iter()
        .next()
        .ok_or_else(|| invalid("No installation targets selected"))?;
    for item in &planned {
        if device(&item.target)? != device(&recovery)? {
            return Err(invalid(
                "Installation targets must share the recovery filesystem",
            ));
        }
    }
    Ok(Plan {
        targets: planned,
        recovery,
        payloads,
        ancestors,
    })
}

fn device(p: &Path) -> io::Result<u64> {
    for p in p.ancestors() {
        if let Some(id) = observed(p)? {
            return Ok(id.0);
        }
    }
    Err(invalid("No existing installation ancestor"))
}

pub fn check_parents(parent: &Path, expected: &Ancestors) -> io::Result<()> {
    safe_path(parent, true)?;
    for p in parent.ancestors() {
        if expected
            .get(p)
            .is_some_and(|id| observed(p).map_or(true, |v| v != *id))
        {
            return Err(invalid(&format!(
                "Installation ancestor changed after preflight: {}",
                p.display()
            )));
        }
    }
    Ok(())
}

pub fn make_parents(
    parent: &Path,
    created: &mut Vec<(PathBuf, (u64, u64))>,
    expected: &mut Ancestors,
) -> io::Result<()> {
    check_parents(parent, expected)?;
    let mut missing = Vec::new();
    for p in parent.ancestors() {
        if existing(p)? {
            break;
        }
        missing.push(p);
    }
    for p in missing.into_iter().rev() {
        let parent = p
            .parent()
            .ok_or_else(|| invalid("Installation parent is missing"))?;
        check_parents(parent, expected)?;
        fs::create_dir(p)?;
        sync_directory(parent)?;
        let id = identity_numbers(p)?;
        expected.insert(p.into(), Some(id));
        created.push((p.into(), id));
    }
    Ok(())
}

fn remember(ancestors: &mut Map<String, Value>, path: &Path) -> io::Result<()> {
    for p in path.ancestors() {
        let name = p
            .to_str()
            .ok_or_else(|| invalid("Installation path is not UTF-8"))?;
        ancestors.insert(name.into(), json!(identity(p)?));
    }
    Ok(())
}

pub fn execute_plan(plan: Plan, home: &Path) -> io::Result<Vec<Option<PathBuf>>> {
    let (owner, _lock) = lock_owner(home)?;
    execute_owned(plan, &owner, home)
}

pub fn execute_owned(
    mut plan: Plan,
    owner: &Path,
    home: &Path,
) -> io::Result<Vec<Option<PathBuf>>> {
    let targets: Vec<_> = plan.targets.iter().map(|p| p.target.as_path()).collect();
    if targets.iter().any(|p| overlaps(p, owner)) || overlaps(&plan.recovery, owner) {
        return Err(invalid(
            "Installation target or recovery overlaps account state",
        ));
    }
    registry::refuse_pending(owner, &targets, home)?;
    registry::check_capacity(owner, home)?;
    let manifests: BTreeMap<_, _> = plan
        .payloads
        .iter()
        .map(|(name, content)| Ok((name, distribution::distribution_manifest(content)?)))
        .collect::<io::Result<_>>()?;
    let mut parents = Vec::new();
    let mut records = Vec::new();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let transaction = plan.recovery.join(format!("transaction-{id}"));
    for (i, item) in plan.targets.iter().enumerate() {
        let target = &item.target;
        let parent = target
            .parent()
            .ok_or_else(|| invalid("Installation target has no parent"))?;
        check_parents(parent, &plan.ancestors)?;
        if observed(target)? != item.previous {
            return Err(invalid("Installation target changed after preflight"));
        }
        let (previous_id, old) = if let Some(expected) = item.previous {
            let (id, manifest) = inspect_tree(target)?;
            if id != format_identity(expected) {
                return Err(invalid("Installation target changed during inspection"));
            }
            (json!(id), manifest)
        } else {
            (Value::Null, Value::Null)
        };
        let new = if plan.payloads.is_empty() {
            let (saved_id, saved_tree) = registry::installed_manifest(owner, target, home)?;
            if previous_id != saved_id
                || !manifest_matches(&old, &saved_tree)
                || !distribution::legacy_binary_modes_match(&old, &saved_tree)
            {
                return Err(invalid(
                    "Installed tree changed; preserve it instead of uninstalling",
                ));
            }
            &Value::Null
        } else {
            manifests
                .get(&item.payload)
                .ok_or_else(|| invalid("Missing installation payload"))?
        };
        records.push(json!({"target":target,"stage":transaction.join(format!("new-{i:04}")),"backup":transaction.join(format!("old-{i:04}")),"disposal":transaction.join(format!("dispose-{i:04}")),"previous_id":previous_id,"stage_id":null,"old":old,"new":new,"phase":"staging"}));
    }
    if records.is_empty() {
        return Err(invalid("No installation targets selected"));
    }
    check_parents(&plan.recovery, &plan.ancestors)?;
    make_parents(&plan.recovery, &mut parents, &mut plan.ancestors)?;
    check_parents(&plan.recovery, &plan.ancestors)?;
    fs::create_dir(&transaction)?;
    sync_directory(&plan.recovery)?;
    let journal = transaction.join("journal.json");
    let mut ancestors = Map::new();
    remember(&mut ancestors, &transaction)?;
    for (p, id) in &plan.ancestors {
        if id.is_some() {
            remember(&mut ancestors, p)?;
        }
    }
    let mut data = json!({"version":1,"id":id,"owner":safe_path(home,true)?,"recovery":plan.recovery,"state":"staging","records":records,"ancestors":ancestors});
    write_bytes(&journal, &json_bytes(&data)?, false)?;
    registry::register(owner, &journal)?;
    let result: io::Result<Vec<Option<PathBuf>>> = (|| {
        stage_and_publish(&mut plan, &mut parents, &mut data, owner, &journal)?;
        transition(&mut data, None, "committed", owner, &journal)?;
        registry::unregister(owner, &journal, home)?;
        plan.targets
            .iter()
            .enumerate()
            .map(|(i, item)| {
                item.previous
                    .map(|_| path(&data["records"][i]["backup"]).map(Path::to_path_buf))
                    .transpose()
            })
            .collect()
    })();
    if result.is_err() {
        let rollback = recovery::restore(&journal, owner, home, false);
        if rollback.is_err() {
            return Err(invalid(&format!(
                "Installation rollback incomplete; preserve recovery paths: {}",
                journal.display()
            )));
        }
        if matches!(rollback, Ok(true)) {
            for (p, id) in parents.iter().rev() {
                if safe_path(p, true).is_ok()
                    && identity_numbers(p).is_ok_and(|current| current == *id)
                {
                    let _ = fs::remove_dir(p);
                }
            }
        }
    }
    result
}

fn transition(
    data: &mut Value,
    index: Option<usize>,
    state: &str,
    owner: &Path,
    journal: &Path,
) -> io::Result<()> {
    if let Some(i) = index {
        data["records"][i]["phase"] = json!(state);
    } else {
        data["state"] = json!(state);
    }
    registry::save_journal(owner, journal, data)
}

fn stage_and_publish(
    plan: &mut Plan,
    parents: &mut Vec<(PathBuf, (u64, u64))>,
    data: &mut Value,
    owner: &Path,
    journal: &Path,
) -> io::Result<()> {
    for (i, item) in plan.targets.iter().enumerate() {
        let parent = item
            .target
            .parent()
            .ok_or_else(|| invalid("Installation target has no parent"))?;
        make_parents(parent, parents, &mut plan.ancestors)?;
        remember(
            data["ancestors"]
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid ancestor inventory"))?,
            parent,
        )?;
        registry::save_journal(owner, journal, data)?;
        if data["records"][i]["new"].is_null() {
            continue;
        }
        let stage = safe_path(path(&data["records"][i]["stage"])?, true)?;
        let parent = stage
            .parent()
            .ok_or_else(|| invalid("Missing transaction directory"))?;
        // Recovery ignores an unbound sibling and only cleans the reserved name after binding it.
        let sibling = parent.join(format!(".stage-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir(&sibling)?;
        data["records"][i]["stage_id"] = json!(identity(&sibling)?);
        registry::save_journal(owner, journal, data)?;
        move_owned(&sibling, &stage)?;
        for (source, relative) in plan
            .payloads
            .get(&item.payload)
            .ok_or_else(|| invalid("Missing installation payload"))?
        {
            let target = stage.join(relative);
            let parent = target
                .parent()
                .ok_or_else(|| invalid("Missing staged file parent"))?;
            safe_path(parent, true)?;
            fs::create_dir_all(parent)?;
            let bytes = read_limited(source, MAX_FILE)?;
            let mut output = File::create_new(&target)?;
            output.write_all(&bytes)?;
            distribution::set_binary_permissions(&output, relative)?;
            output.sync_all()?;
        }
        let record = &data["records"][i];
        if !matches(&stage, &record["stage_id"], &record["new"]) {
            return Err(invalid("Installation source changed during staging"));
        }
        if let Some(dirs) = record["new"]["directories"].as_array() {
            for dir in dirs.iter().rev() {
                sync_directory(&stage.join(path(dir)?))?;
            }
        }
        sync_directory(&stage)?;
        transition(data, Some(i), "staged", owner, journal)?;
    }
    transition(data, None, "staged", owner, journal)?;
    for (i, item) in plan.targets.iter().enumerate() {
        let target = &item.target;
        let record = data["records"][i].clone();
        let parent = target
            .parent()
            .ok_or_else(|| invalid("Missing installation parent"))?;
        if item.previous.is_some() {
            transition(data, Some(i), "old-intent", owner, journal)?;
            check_parents(parent, &plan.ancestors)?;
            if !matches(target, &record["previous_id"], &record["old"]) {
                return Err(invalid("Previous installation changed before backup"));
            }
            move_owned(target, path(&record["backup"])?)?;
            transition(data, Some(i), "old-moved", owner, journal)?;
        } else if existing(target)? {
            return Err(invalid("Installation target became occupied"));
        }
        if record["new"].is_null() {
            continue;
        }
        transition(data, Some(i), "new-intent", owner, journal)?;
        check_parents(parent, &plan.ancestors)?;
        move_owned(path(&record["stage"])?, target)?;
        if !matches(target, &record["stage_id"], &record["new"]) {
            return Err(invalid("Published installation bytes changed"));
        }
        transition(data, Some(i), "published", owner, journal)?;
    }
    Ok(())
}
