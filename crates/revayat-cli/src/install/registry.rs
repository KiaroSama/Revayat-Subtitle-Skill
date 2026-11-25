use super::{files::*, journal::*};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

pub const CAPACITY: usize = 1000;
pub const LEGACY_CAPACITY: usize = CAPACITY + 1;
const KEY: &str = "registry.key";
const REGISTRY: &str = "pending.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEntry {
    pub path: PathBuf,
    pub transaction_id: Identity,
    pub digests: Vec<String>,
    pub active: bool,
}

#[derive(Debug)]
pub struct InstallRegistry {
    pub records: Vec<RegistryEntry>,
}

impl InstallRegistry {
    pub fn load(root: &Path) -> io::Result<Self> {
        let path = safe_path(&root.join(REGISTRY), false)?;
        let key = key(root)?;
        let authentication =
            || invalid("Installation pending registry authentication failed; preserve state");
        if !existing(&path)? {
            return if key.is_some() {
                Err(authentication())
            } else {
                Ok(Self {
                    records: Vec::new(),
                })
            };
        }
        let value = read_json(&path)?;
        let records = match key {
            Some(key) => {
                if !object_fields(&value, &["records", "hmac"]) {
                    return Err(authentication());
                }
                let tag = value["hmac"]
                    .as_str()
                    .filter(|s| is_hex(s, 64))
                    .ok_or_else(authentication)?;
                mac(&key, &value["records"])?
                    .verify_slice(&hex::decode(tag).map_err(io::Error::other)?)
                    .map_err(|_| authentication())?;
                value["records"].clone()
            }
            None if value.is_array() => value,
            None => return Err(authentication()),
        };
        let schema = || invalid("Installation pending registry is invalid; preserve state");
        if !records
            .as_array()
            .is_some_and(|r| r.len() <= LEGACY_CAPACITY)
        {
            return Err(schema());
        }
        let records: Vec<RegistryEntry> = serde_json::from_value(records).map_err(|_| schema())?;
        let mut seen = HashSet::with_capacity(records.len());
        for entry in &records {
            if !canonical(&entry.path)
                || !seen.insert(path_key(&entry.path))
                || !valid_identity(&json!(entry.transaction_id))
                || !(1..=2).contains(&entry.digests.len())
                || !entry.digests.iter().all(|s| is_hex(s, 64))
            {
                return Err(schema());
            }
        }
        Ok(Self { records })
    }

    fn save(&self, root: &Path) -> io::Result<()> {
        let records = serde_json::to_value(&self.records)?;
        let existing_key = key(root)?;
        let key = match existing_key {
            Some(key) => key,
            None => {
                let mut key = [0; 32];
                getrandom::fill(&mut key).map_err(io::Error::other)?;
                key
            }
        };
        let hmac = hex::encode(mac(&key, &records)?.finalize().into_bytes());
        let data = json_bytes(&json!({"records": records, "hmac": hmac}))?;
        // Key-first publication prevents accepting a downgrade after an interrupted migration.
        if existing_key.is_none() {
            write_bytes(&root.join(KEY), &key, false)?;
        }
        write_bytes(&root.join(REGISTRY), &data, true)
    }

    fn entry_index(&self, journal: &Path) -> io::Result<usize> {
        self.records
            .iter()
            .position(|row| row.path == journal)
            .ok_or_else(changed)
    }

    fn verify(&self, journal: &Path) -> io::Result<(usize, String, Vec<u8>)> {
        let journal = safe_path(journal, false)?;
        let i = self.entry_index(&journal)?;
        let parent = journal.parent().ok_or_else(changed)?;
        let bytes = read_limited(&journal, MAX_FILE)?;
        let observed = digest(&bytes);
        let entry = &self.records[i];
        if identity(parent)? != entry.transaction_id || !entry.digests.contains(&observed) {
            return Err(changed());
        }
        Ok((i, observed, bytes))
    }

    fn load_journal(&self, journal: &Path, home: &Path) -> io::Result<Value> {
        let (_, _, bytes) = self.verify(journal)?;
        validate_journal(
            &safe_path(journal, false)?,
            home,
            serde_json::from_slice(&bytes)?,
        )
    }

    fn can_retire(&self, entry: &RegistryEntry, home: &Path) -> io::Result<bool> {
        if entry.active {
            return Ok(false);
        }
        if !existing(&safe_path(&entry.path, false)?)? {
            return Ok(true);
        }
        let data = self.load_journal(&entry.path, home)?;
        Ok(data["state"] == "committed"
            || data["state"] == "restored" && !disposals_pending(&data)?)
    }

    fn evict_one(&mut self, home: &Path, preserve: Option<&Path>) -> bool {
        let index = self.records.iter().position(|row| {
            preserve != Some(row.path.as_path()) && self.can_retire(row, home).unwrap_or(false)
        });
        if let Some(index) = index {
            self.records.remove(index);
        }
        index.is_some()
    }
}

fn changed() -> io::Error {
    invalid("Unregistered or changed installation transaction instructions; preserve state")
}

fn key(root: &Path) -> io::Result<Option<[u8; 32]>> {
    let path = safe_path(&root.join(KEY), false)?;
    if !existing(&path)? {
        return Ok(None);
    }
    read_limited(&path, 32)?
        .try_into()
        .map(Some)
        .map_err(|_| invalid("Installation registry authentication key is invalid"))
}

fn mac(key: &[u8], records: &Value) -> io::Result<Hmac<Sha256>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(io::Error::other)?;
    mac.update(b"revayat-install-registry-v1\0");
    mac.update(&serde_json::to_vec(records)?);
    Ok(mac)
}

pub fn registry(root: &Path) -> io::Result<Vec<RegistryEntry>> {
    Ok(InstallRegistry::load(root)?.records)
}

pub fn registered(root: &Path, journal: &Path) -> io::Result<RegistryEntry> {
    let registry = InstallRegistry::load(root)?;
    Ok(registry.records[registry.verify(journal)?.0].clone())
}

pub fn load_registered(root: &Path, journal: &Path, home: &Path) -> io::Result<Value> {
    InstallRegistry::load(root)?.load_journal(journal, home)
}

pub(super) fn check_capacity(root: &Path, home: &Path) -> io::Result<()> {
    let mut registry = InstallRegistry::load(root)?;
    while registry.records.len() >= CAPACITY {
        if !registry.evict_one(home, None) {
            return Err(invalid(
                "Installation pending registry is full; recover active transactions first",
            ));
        }
    }
    Ok(())
}

pub fn register(root: &Path, journal: &Path) -> io::Result<()> {
    let journal = safe_path(journal, false)?;
    let home = root
        .parent()
        .ok_or_else(|| invalid("Missing installation account home"))?;
    let mut registry = InstallRegistry::load(root)?;
    if registry.entry_index(&journal).is_ok() {
        return Err(invalid("Installation transaction is already registered"));
    }
    while registry.records.len() >= CAPACITY {
        if !registry.evict_one(home, None) {
            return Err(invalid(
                "Installation pending registry is full; recover active transactions first",
            ));
        }
    }
    let parent = journal
        .parent()
        .ok_or_else(|| invalid("Missing transaction parent"))?;
    let transaction_id = identity(parent)?;
    let bytes = read_limited(&journal, MAX_FILE)?;
    validate_journal(&journal, home, serde_json::from_slice(&bytes)?)?;
    registry.records.push(RegistryEntry {
        transaction_id,
        path: journal,
        digests: vec![digest(&bytes)],
        active: true,
    });
    registry.save(root)
}

pub fn save_journal(root: &Path, journal: &Path, data: &Value) -> io::Result<()> {
    let mut registry = InstallRegistry::load(root)?;
    let (i, old, _) = registry.verify(journal)?;
    let bytes = json_bytes(data)?;
    let new = digest(&bytes);
    registry.records[i].digests = if old == new {
        vec![old.clone()]
    } else {
        vec![old.clone(), new.clone()]
    };
    registry.save(root)?;
    let outcome = write_bytes(journal, &bytes, true);
    // An uncertain replacement can leave either exact revision on disk.
    if let Ok(observed) = read_limited(journal, MAX_FILE).map(|bytes| digest(&bytes))
        && (observed == old || observed == new)
    {
        registry.records[i].digests = vec![observed];
        let _ = registry.save(root);
    }
    outcome
}

pub fn unregister(root: &Path, journal: &Path, home: &Path) -> io::Result<()> {
    let journal = safe_path(journal, false)?;
    let mut registry = InstallRegistry::load(root)?;
    let (i, _, bytes) = registry.verify(&journal)?;
    let data = validate_journal(&journal, home, serde_json::from_slice(&bytes)?)?;
    let complete =
        data["state"] == "committed" || data["state"] == "restored" && !disposals_pending(&data)?;
    registry.records[i].active = !complete;
    while registry.records.len() > CAPACITY
        && (registry.evict_one(home, Some(&journal)) || registry.evict_one(home, None))
    {}
    registry.save(root)
}

pub fn refuse_pending(root: &Path, targets: &[&Path], home: &Path) -> io::Result<()> {
    let registry = InstallRegistry::load(root)?;
    for entry in registry.records.iter().filter(|row| row.active) {
        let data = registry.load_journal(&entry.path, home)?;
        if data["state"] == "committed" || data["state"] == "restored" {
            let complete = data["state"] == "committed" || !disposals_pending(&data)?;
            if complete {
                unregister(root, &entry.path, home)?;
            }
        } else {
            for record in records(&data)? {
                let previous = path(&record["target"])?;
                if targets.iter().any(|target| overlaps(target, previous)) {
                    return Err(invalid(&format!(
                        "Interrupted installation needs explicit --recover: {}",
                        entry.path.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn installed_manifest(
    root: &Path,
    target: &Path,
    home: &Path,
) -> io::Result<(Value, Value)> {
    let registry = InstallRegistry::load(root)?;
    for entry in registry.records.iter().rev() {
        if !existing(&safe_path(&entry.path, false)?)? {
            continue;
        }
        let data = registry.load_journal(&entry.path, home)?;
        if data["state"] != "committed" {
            continue;
        }
        for record in records(&data)? {
            if path_key(path(&record["target"])?) == path_key(target) {
                if !record["new"].is_null() {
                    return Ok((record["stage_id"].clone(), record["new"].clone()));
                }
                return Err(invalid(
                    "The latest installation transaction already removed this target",
                ));
            }
        }
    }
    Err(invalid(
        "No authenticated installation history for this target; preserve it",
    ))
}
