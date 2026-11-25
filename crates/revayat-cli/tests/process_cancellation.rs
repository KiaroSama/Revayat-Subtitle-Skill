use anyhow::{Result, anyhow};
use revayat_core::process_control::{RunError, RunOptions, request_cancel, run_owned};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "REVAYAT_CANCELLATION_TEST_ROLE";
const MARKER: &str = "REVAYAT_CANCELLATION_TEST_MARKER";

fn argv() -> Result<Vec<String>> {
    Ok(vec![
        std::env::current_exe()?
            .to_str()
            .ok_or_else(|| anyhow!("test path"))?
            .into(),
        "--exact".into(),
        "cancellation_child_fixture".into(),
        "--nocapture".into(),
    ])
}

fn wait_ready(marker: &Path) -> Result<()> {
    let start = Instant::now();
    while !marker.with_extension("ready").exists() {
        if start.elapsed() > Duration::from_secs(4) {
            return Err(anyhow!("owned descendant did not start"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn stopped(marker: &Path) -> Result<()> {
    let lock = fs::OpenOptions::new().read(true).write(true).open(marker)?;
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            Err(fs::TryLockError::WouldBlock) if start.elapsed() < Duration::from_secs(2) => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(anyhow!("owned descendant survived cancellation"));
            }
        }
    }
}

// Re-enter an isolated process so sticky cancellation cannot affect parallel tests.
#[test]
fn cancellation_child_fixture() -> Result<()> {
    let Ok(role) = std::env::var(ROLE) else {
        return Ok(());
    };
    let marker = PathBuf::from(std::env::var_os(MARKER).ok_or_else(|| anyhow!("marker"))?);
    match role.as_str() {
        "descendant" => {
            let lock = fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&marker)?;
            lock.try_lock()?;
            fs::write(marker.with_extension("ready"), b"ready")?;
            thread::sleep(Duration::from_secs(12));
        }
        "leader" => {
            let args = argv()?;
            let mut child = Command::new(&args[0])
                .args(&args[1..])
                .env(ROLE, "descendant")
                .stdin(Stdio::null())
                .spawn()?;
            let _ = child.wait()?;
        }
        "owner" => {
            let wake = marker.clone();
            let canceller = thread::spawn(move || -> Result<()> {
                wait_ready(&wake)?;
                request_cancel();
                Ok(())
            });
            let env = BTreeMap::from([
                (ROLE.into(), "leader".into()),
                (
                    MARKER.into(),
                    marker
                        .to_str()
                        .ok_or_else(|| anyhow!("marker path"))?
                        .into(),
                ),
            ]);
            let outcome = run_owned(
                &argv()?,
                &RunOptions {
                    timeout: Duration::from_secs(10),
                    env: Some(&env),
                    ..RunOptions::default()
                },
            );
            canceller
                .join()
                .map_err(|_| anyhow!("cancellation thread"))??;
            assert!(matches!(outcome, Err(RunError::Cancelled)), "{outcome:?}");
            stopped(&marker)?;
            assert!(matches!(
                run_owned(
                    &["missing-executable-must-not-be-started".into()],
                    &RunOptions::default()
                ),
                Err(RunError::Cancelled)
            ));
        }
        _ => return Err(anyhow!("unknown fixture role")),
    }
    Ok(())
}

#[test]
fn cooperative_cancellation_terminates_and_reaps_the_owned_tree() -> Result<()> {
    let root = tempfile::tempdir()?;
    let args = argv()?;
    let output = Command::new(&args[0])
        .args(&args[1..])
        .env(ROLE, "owner")
        .env(MARKER, root.path().join("child.lock"))
        .stdin(Stdio::null())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn cli_interrupt_returns_130_after_stopping_its_owned_descendant() -> Result<()> {
    use rustix::process::{Pid, Signal, kill_process};
    use std::os::unix::process::CommandExt;

    let root = tempfile::tempdir()?;
    let marker = root.path().join("child.lock");
    let mut child = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"))
        .current_dir(root.path())
        .env("REVAYAT_LOG_DIR", root.path().join("logs"))
        .env(ROLE, "leader")
        .env(MARKER, &marker)
        .args(["supervise", "--timeout", "10", "--"])
        .args(argv()?)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_ready(&marker)?;
    let pid = Pid::from_raw(i32::try_from(child.id())?).ok_or_else(|| anyhow!("CLI PID"))?;
    kill_process(pid, Signal::INT)?;
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(6) {
            child.kill()?;
            let _ = child.wait();
            return Err(anyhow!(
                "CLI did not finish cancellation within cleanup budget"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert_eq!(
        output.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stopped(&marker)?;
    let log = fs::read_dir(root.path().join("logs"))?
        .next()
        .ok_or_else(|| anyhow!("missing log"))??;
    assert!(fs::read_to_string(log.path())?.contains("exit_code=130"));
    Ok(())
}
