use super::{
    distribution::{self, AGENTS},
    files::{invalid, lock_owner, safe_path},
    recovery, transaction,
};
use clap::Args;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Args, Debug)]
#[command(about = "Install, remove, or recover Revayat skills or a plugin bundle")]
pub struct InstallArgs {
    #[arg(long, default_value = "all", value_parser = clap::builder::PossibleValuesParser::new(AGENTS.iter().copied().chain(["all"])))]
    pub agent: String,
    #[arg(long, default_value = "user", value_parser = ["user", "project"])]
    pub scope: String,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long, help = "Checkout that contains the skill distribution")]
    pub source: Option<PathBuf>,
    #[arg(long, conflicts_with = "recover", requires = "recovery_dir")]
    pub destination: Option<PathBuf>,
    #[arg(long, requires = "destination", conflicts_with = "recover")]
    pub plugin: bool,
    #[arg(long, conflicts_with_all = ["recover", "uninstall"])]
    pub force: bool,
    #[arg(long, conflicts_with = "recover")]
    pub dry_run: bool,
    #[arg(long)]
    pub recovery_dir: Option<PathBuf>,
    #[arg(long, requires = "recovery_dir", conflicts_with = "uninstall")]
    pub recover: Option<PathBuf>,
    #[arg(
        long,
        help = "Remove an unchanged Revayat installation and retain its backup"
    )]
    pub uninstall: bool,
}

pub fn run_install(args: InstallArgs) -> anyhow::Result<()> {
    let home =
        dirs::home_dir().ok_or_else(|| invalid("Cannot locate installation account home"))?;
    let checkout = match (&args.source, &args.recover) {
        (_, Some(_)) => PathBuf::new(),
        (Some(path), _) => safe_path(path, true)?,
        _ => locate_checkout()?,
    };
    let report = install(&checkout, &home, args)?;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &report)?;
    writeln!(stdout)?;
    Ok(())
}

fn locate_checkout() -> io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let executable = std::env::current_exe()?;
    let check = |p: &&Path| p.join("skills/revayat-subtitle/SKILL.md").is_file();
    cwd.ancestors()
        .find(check)
        .or_else(|| executable.ancestors().skip(1).find(check))
        .map(Path::to_path_buf)
        .ok_or_else(|| invalid("Cannot find the checkout; run from the checkout or pass --source"))
}

pub fn install(root: &Path, home: &Path, args: InstallArgs) -> io::Result<Value> {
    if let Some(journal) = args.recover {
        let recovery = args
            .recovery_dir
            .filter(|_| {
                !args.uninstall
                    && !args.force
                    && !args.dry_run
                    && !args.plugin
                    && args.destination.is_none()
            })
            .ok_or_else(|| {
                invalid("--recover needs --recovery-dir and cannot combine installation flags")
            })?;
        recovery::recover(&journal, &recovery, home)?;
        return Ok(json!({"state":"restored", "journal":journal}));
    }
    let custom = args.destination.is_some();
    if custom && args.recovery_dir.is_none() || args.plugin && !custom {
        return Err(invalid(
            "A custom destination or plugin requires --destination and --recovery-dir",
        ));
    }
    let mut targets: Vec<_> = args.destination.into_iter().collect();
    let mut skipped = Vec::new();
    if !custom {
        let base = match args.scope.as_str() {
            "user" => home.to_path_buf(),
            "project" => args.path,
            _ => return Err(invalid("Installation scope must be user or project")),
        };
        let base = safe_path(&base, true)?;
        if !base.is_dir() {
            return Err(invalid("Installation base directory does not exist"));
        }
        let agents = if args.agent == "all" {
            AGENTS
        } else {
            &[args.agent.as_str()]
        };
        for agent in agents {
            let target = distribution::destination(agent, &args.scope, &base)
                .ok_or_else(|| invalid("Unknown installation agent"))?;
            if args.agent == "all" && args.scope == "user" {
                let agent_root = target
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| invalid("Missing installation agent root"))?;
                if !safe_path(agent_root, true)?.is_dir()
                    && !(*agent == "codex" && safe_path(&base.join(".codex"), true)?.is_dir())
                {
                    skipped.push(*agent);
                    continue;
                }
            }
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }
    if targets.is_empty() {
        return Err(invalid(
            "No installed agents found; select --agent or --destination explicitly",
        ));
    }
    let plan = if args.uninstall {
        transaction::plan_uninstall(
            root,
            &targets,
            args.plugin,
            args.recovery_dir.as_deref(),
            !custom,
        )?
    } else {
        transaction::plan_install(
            root,
            &targets,
            args.plugin,
            args.force,
            args.recovery_dir.as_deref(),
            !custom,
        )?
    };
    let targets: Vec<_> = plan
        .targets
        .iter()
        .map(|item| item.target.clone())
        .collect();
    if args.dry_run {
        return Ok(
            json!({"state":"dry-run", "uninstall":args.uninstall, "targets":targets, "skipped":skipped, "recovery":plan.recovery}),
        );
    }
    let (owner, _lock) = lock_owner(home)?;
    let backups = transaction::execute_owned(plan, &owner, home)?;
    let changes: Vec<_> = targets
        .into_iter()
        .zip(backups)
        .map(|(target, backup)| json!({"target":target,"backup":backup}))
        .collect();
    Ok(
        json!({"state": if args.uninstall { "uninstalled" } else { "installed" }, "changes":changes, "skipped":skipped}),
    )
}
