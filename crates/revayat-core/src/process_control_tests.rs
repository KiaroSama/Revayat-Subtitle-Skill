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
    let executable = std::env::current_exe()?;
    let command = [
        executable
            .to_str()
            .ok_or("Non-UTF-8 test executable path")?,
        "--exact",
        IDLE_HELPER,
        "--nocapture",
    ]
    .map(str::to_owned);
    let env = [(IDLE_ROLE.into(), "leader".into())].into();
    let options = RunOptions {
        cwd: Some(temp.path()),
        timeout: Duration::from_secs(5),
        idle_timeout: Some(Duration::from_secs(2)),
        env: Some(&env),
        ..RunOptions::default()
    };
    assert!(matches!(
        run_owned(&command, &options),
        Err(RunError::Timeout { wall: false, .. })
    ));
    assert!(temp.path().join("launched").is_file());
    assert!(!temp.path().join("escaped").exists());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join("descendant.lock"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while let Err(error) = lock.try_lock() {
        if std::time::Instant::now() >= deadline {
            return Err(error.into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

const IDLE_HELPER: &str = "process_control::tests::idle_cancellation_helper";
const IDLE_ROLE: &str = "REVAYAT_IDLE_TEST_ROLE";

#[test]
fn idle_cancellation_helper() -> Result<(), Box<dyn Error>> {
    match std::env::var(IDLE_ROLE).as_deref() {
        Ok("leader") => {
            let status = std::process::Command::new(std::env::current_exe()?)
                .args(["--exact", IDLE_HELPER, "--nocapture"])
                .env(IDLE_ROLE, "descendant")
                .status()?;
            assert!(status.success());
        }
        Ok("descendant") => {
            let lock = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open("descendant.lock")?;
            lock.lock()?;
            fs::write("launched", b"")?;
            std::thread::sleep(Duration::from_secs(8));
            fs::write("escaped", b"")?;
            lock.unlock()?;
        }
        _ => (),
    }
    Ok(())
}

fn compatibility_timeout(wall: f64, idle: f64, cause: &str) -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let command = vec![
        std::env::current_exe()?
            .to_str()
            .ok_or("Non-UTF-8 test executable path")?
            .to_owned(),
        "--exact".into(),
        IDLE_HELPER.into(),
        "--nocapture".into(),
    ];
    let error = run(
        command,
        Some(json!({"$path": temp.path()})),
        wall,
        Some(idle),
        None,
        Some(json!({IDLE_ROLE: "leader"})),
        false,
    )
    .expect_err("compatibility wrapper must retain timeout failures");
    assert!(error.starts_with("subprocess.TimeoutExpired: "), "{error}");
    assert!(
        error.contains(&format!("{cause} deadline expired after ")),
        "{error}"
    );
    let elapsed: f64 = error
        .rsplit_once(" after ")
        .ok_or("elapsed timeout diagnostic")?
        .1
        .strip_suffix(" seconds")
        .ok_or("elapsed seconds suffix")?
        .parse()?;
    assert!((2.0..5.0).contains(&elapsed), "{error}");
    assert!(temp.path().join("launched").is_file());
    assert!(!temp.path().join("escaped").exists());
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(temp.path().join("descendant.lock"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while let Err(error) = lock.try_lock() {
        if std::time::Instant::now() >= deadline {
            return Err(error.into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn compatibility_idle_timeout_preserves_cause_and_cleanup() -> Result<(), Box<dyn Error>> {
    compatibility_timeout(5.0, 2.0, "idle")
}

#[test]
fn compatibility_wall_timeout_preserves_cause_and_cleanup() -> Result<(), Box<dyn Error>> {
    compatibility_timeout(2.0, 5.0, "wall")
}

#[test]
fn migration_regression() -> Result<(), String> {
    assert_eq!(
        checked_environment(serde_json::json!({"OUT":"فارسی"}))?,
        serde_json::json!({"OUT":"فارسی"})
    );
    Ok(())
}
