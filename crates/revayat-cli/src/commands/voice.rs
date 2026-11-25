use super::{TEXT_LIMIT, core_error, write_json};
use crate::cli::{LintArgs, LookupArgs};
use anyhow::{Result, anyhow, ensure};
use revayat_core::{
    lookup as terminology, persian_lint,
    runtime::{diagnostic::redact, files},
};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

fn read_text(path: &Path) -> Result<String> {
    let bytes = if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .take(TEXT_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= TEXT_LIMIT,
            "Stdin exceeds the supported byte limit"
        );
        bytes
    } else {
        files::read_limited(path, TEXT_LIMIT).map_err(core_error)?
    };
    Ok(String::from_utf8(bytes)?)
}

fn replace_text(path: &Path, text: &str, permissions: fs::Permissions) -> Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tempfile = tempfile::NamedTempFile::new_in(parent)?;
    tempfile.write_all(text.as_bytes())?;
    tempfile.as_file().set_permissions(permissions)?;
    tempfile.as_file().sync_all()?;
    tempfile.persist(path)?;
    Ok(())
}

pub(super) fn lint(args: LintArgs) -> Result<i32> {
    let stdin = PathBuf::from("-");
    let inputs = if args.files.is_empty() {
        std::slice::from_ref(&stdin)
    } else {
        &args.files
    };
    ensure!(
        !args.in_place || !inputs.iter().any(|path| path == &stdin),
        "--in-place requires file paths, not stdin"
    );
    ensure!(
        inputs.iter().filter(|path| *path == &stdin).count() <= 1,
        "Stdin can be read only once"
    );
    let mut reports = Vec::new();
    let mut has_findings = false;
    for path in inputs {
        let permissions = if args.in_place {
            let metadata = fs::symlink_metadata(path)?;
            ensure!(
                metadata.is_file() && !files::linked(&metadata),
                "--in-place requires a regular file, not a link"
            );
            Some(metadata.permissions())
        } else {
            None
        };
        let text = read_text(path)?;
        let normalized =
            persian_lint::normalize(json!(text), args.digits.clone()).map_err(core_error)?;
        let normalized = &normalized["$tuple"];
        let fixed = normalized[0]
            .as_str()
            .ok_or_else(|| anyhow!("Linter returned an invalid normalization result"))?;
        let stats = &normalized[1];
        let total = stats
            .as_object()
            .into_iter()
            .flat_map(|rules| rules.values())
            .filter_map(Value::as_u64)
            .sum::<u64>();
        let suggestions =
            persian_lint::suggestions(json!(if args.fix { fixed } else { &text }), args.style)
                .map_err(core_error)?;
        let suggestions = suggestions
            .as_array()
            .ok_or_else(|| anyhow!("Linter returned invalid suggestions"))?;
        if args.fix {
            if let Some(permissions) = permissions {
                if fixed != text {
                    replace_text(path, fixed, permissions)?;
                }
                eprintln!(
                    "fixed: {} ({total} changes)",
                    redact(&path.to_string_lossy())
                );
            } else {
                io::stdout().lock().write_all(fixed.as_bytes())?;
            }
            if !suggestions.is_empty() {
                eprintln!(
                    "\n{} item(s) need judgment (run --check for details)",
                    suggestions.len()
                );
            }
            continue;
        }
        let changes = persian_lint::line_changes(json!(text), json!(fixed)).map_err(core_error)?;
        has_findings |= total > 0 || !suggestions.is_empty();
        if args.json {
            let mut report =
                json!({"stats": stats, "changes": changes, "suggestions": suggestions});
            if inputs.len() > 1 {
                report["file"] = json!(path.to_string_lossy());
            }
            reports.push(report);
        } else {
            let mut stdout = io::stdout().lock();
            if inputs.len() > 1 {
                writeln!(stdout, "{}:", path.display())?;
            }
            lint_report(&mut stdout, stats, total, &changes, suggestions)?;
        }
    }
    if args.json {
        if let [report] = reports.as_slice() {
            write_json(report)?;
        } else {
            write_json(&Value::Array(reports))?;
        }
    }
    io::stdout().lock().flush()?;
    Ok(i32::from(has_findings))
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn lint_report(
    out: &mut impl Write,
    stats: &Value,
    total: u64,
    changes: &Value,
    suggestions: &[Value],
) -> io::Result<()> {
    if total == 0 && suggestions.is_empty() {
        return writeln!(out, "clean - no mechanics issues found.");
    }
    let changes = changes.as_array().map_or(&[][..], Vec::as_slice);
    writeln!(
        out,
        "persian_lint: {total} auto-fixable change(s) across {} line(s)\n",
        changes.len()
    )?;
    let mut rules: Vec<_> = stats
        .as_object()
        .into_iter()
        .flat_map(|stats| stats.iter())
        .collect();
    rules.sort_by_key(|(_, n)| std::cmp::Reverse(n.as_u64().unwrap_or(0)));
    for (rule, n) in rules {
        writeln!(out, "  {:>4}  {rule}", n.as_u64().unwrap_or(0))?;
    }
    for change in changes.iter().take(40) {
        writeln!(
            out,
            "\n  line {}:\n    - {}\n    + {}",
            change["line"],
            field(change, "before"),
            field(change, "after")
        )?;
    }
    if !suggestions.is_empty() {
        writeln!(
            out,
            "\n  {} suggestion(s) needing human judgment:",
            suggestions.len()
        )?;
        for suggestion in suggestions.iter().take(30) {
            writeln!(
                out,
                "    ? {}: ...{}...  {}",
                field(suggestion, "type"),
                field(suggestion, "context"),
                field(suggestion, "hint")
            )?;
        }
    }
    Ok(())
}

const BANKS: [(&str, &str, &str); 2] = [
    ("idioms", "idioms.tsv", "IDIOMS"),
    ("glossary", "glossary.tsv", "GLOSSARY / COLLOCATIONS"),
];

fn bundled_data() -> Option<PathBuf> {
    let executable = env::current_exe().ok();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    executable
        .as_deref()
        .and_then(Path::parent)
        .into_iter()
        .flat_map(Path::ancestors)
        .chain(source.ancestors())
        .flat_map(|root| [root.join("skills/revayat-subtitle"), root.to_owned()])
        .flat_map(|root| {
            [
                root.join("voice/persian/scripts/data"),
                root.join("voice/persian/data"),
            ]
        })
        .find(|path| path.is_dir())
}

fn lookup_dir(text: &str, reverse: bool, path: &Path) -> Result<Value> {
    let path = path.canonicalize()?;
    ensure!(path.is_dir(), "--data-dir requires a directory");
    let find = if reverse {
        terminology::find_fa
    } else {
        terminology::find_en
    };
    let mut result = serde_json::Map::new();
    for (label, filename, _) in BANKS {
        let path = path.join(filename);
        let mut records = Vec::new();
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !files::linked(&metadata) && metadata.len() <= TEXT_LIMIT,
                    "Lookup bank {filename} must be a regular file within the supported byte limit"
                );
                let path = path
                    .to_str()
                    .ok_or_else(|| anyhow!("Lookup data path is not valid UTF-8"))?;
                let rows = terminology::load(json!(path)).map_err(core_error)?;
                let hits = terminology::dedup(find(rows, json!(text)).map_err(core_error)?)
                    .map_err(core_error)?;
                for row in hits.as_array().into_iter().flatten() {
                    let row = &row["$tuple"];
                    records.push(json!({"en":row[0],"fa":row[1],"note":row[2]}));
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        result.insert(label.into(), Value::Array(records));
    }
    Ok(Value::Object(result))
}

pub(super) fn lookup(args: LookupArgs) -> Result<i32> {
    let text = match args.file_or_query {
        None => read_text(Path::new("-"))?,
        Some(input) if input == "-" => read_text(Path::new("-"))?,
        Some(input) => match fs::metadata(&input) {
            Ok(_) => read_text(Path::new(&input))?,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::InvalidFilename
                ) =>
            {
                input
            }
            Err(error) => return Err(error.into()),
        },
    };
    let report = match args.data_dir.or_else(bundled_data) {
        Some(path) => lookup_dir(&text, args.reverse, &path)?,
        None => terminology::lookup_text(&text, args.reverse).map_err(core_error)?,
    };
    if args.json {
        write_json(&report)?;
        return Ok(0);
    }
    let total: usize = report
        .as_object()
        .into_iter()
        .flat_map(|banks| banks.values())
        .filter_map(Value::as_array)
        .map(Vec::len)
        .sum();
    let mut stdout = io::stdout().lock();
    if total == 0 {
        writeln!(stdout, "(no idiom / glossary terms found in source)")?;
    } else {
        let noun = if total == 1 { "entry" } else { "entries" };
        writeln!(stdout, "# Terminology brief ({total} {noun})\n")?;
        for (label, _, title) in BANKS {
            let Some(rows) = report[label].as_array().filter(|rows| !rows.is_empty()) else {
                continue;
            };
            writeln!(stdout, "## {title}")?;
            for row in rows {
                let note = field(row, "note");
                write!(stdout, "- {}  ->  {}", field(row, "en"), field(row, "fa"))?;
                if !note.is_empty() {
                    write!(stdout, "   ({note})")?;
                }
                writeln!(stdout)?;
            }
            writeln!(stdout)?;
        }
    }
    stdout.flush()?;
    Ok(0)
}
