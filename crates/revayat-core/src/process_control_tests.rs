use super::*;
use std::{error::Error, fs, time::Duration};

#[test]
fn captures_separate_streams_inside_selected_directory() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let argv = if cfg!(windows) {
        vec!["cmd.exe", "/d", "/c", "type input.txt & echo stderr 1>&2"]
    } else {
        vec!["sh", "-c", "cat input.txt; printf stderr >&2"]
    };
    fs::write(temp.path().join("input.txt"), "فارسی")?;
    let command: Vec<_> = argv.into_iter().map(str::to_owned).collect();
    let options = RunOptions {
        cwd: Some(temp.path()),
        timeout: Duration::from_secs(3),
        ..RunOptions::default()
    };
    let out = run_owned(&command, &options)?;
    assert_eq!(out.stdout, "فارسی".as_bytes());
    assert!(out.stderr.starts_with(b"stderr"));
    assert!(matches!(
        run_owned(
            &command,
            &RunOptions {
                max_output: 1,
                ..options
            }
        ),
        Err(RunError::Invalid(_))
    ));
    Ok(())
}

#[test]
fn idle_expiry_kills_descendant_before_it_can_write() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let program = if cfg!(windows) { "python" } else { "python3" };
    let leader = "import subprocess, sys, time; subprocess.Popen([sys.executable, '-I', '-c', 'import time; time.sleep(8); open(\"escaped\", \"w\").close()']); open('launched', 'w').close(); print('ready', flush=True); time.sleep(8)";
    let command = [program, "-I", "-c", leader].map(str::to_owned);
    let options = RunOptions {
        cwd: Some(temp.path()),
        timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(2)),
        ..RunOptions::default()
    };
    assert!(matches!(
        run_owned(&command, &options),
        Err(RunError::Timeout { wall: false, .. })
    ));
    assert!(temp.path().join("launched").is_file());
    assert!(!temp.path().join("escaped").exists());
    Ok(())
}

#[test]
fn migration_regression() -> Result<(), String> {
    assert_eq!(
        checked_environment(serde_json::json!({"OUT":"فارسی"}))?,
        serde_json::json!({"OUT":"فارسی"})
    );
    Ok(())
}
