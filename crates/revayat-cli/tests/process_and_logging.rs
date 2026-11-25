use anyhow::{Result, anyhow};
use revayat_core::{
    process_control::{RunError, RunOptions, duration, exit_code, run_owned, validate_environment},
    runtime::{LogLevel, OperationalLog, diagnostic, files},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::{Write, stdout},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "REVAYAT_TEST_CHILD_ROLE";
const MARKER: &str = "REVAYAT_TEST_CHILD_MARKER";
const PRIVATE: &str = "fictional source line never copy to diagnostics";

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_revayat-subtitle"));
    command
        .current_dir(root)
        .env("REVAYAT_LOG_DIR", root.join("logs"))
        .env_remove("REVAYAT_LOG_LEVEL")
        .stdin(Stdio::null());
    command
}

fn child() -> Result<Vec<String>> {
    let exe = std::env::current_exe()?
        .to_str()
        .ok_or_else(|| anyhow!("test executable is not UTF-8"))?
        .to_owned();
    Ok(vec![
        exe,
        "--exact".into(),
        "supervise_forwards_utf8_and_real_child_status".into(),
        "--nocapture".into(),
    ])
}

fn wait_for_marker(path: &Path) {
    let ready = Instant::now();
    while !path.with_extension("ready").is_file() && ready.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        path.with_extension("ready").is_file(),
        "descendant never started"
    );
}

fn child_env(role: &str, marker: Option<&Path>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([(ROLE.into(), role.into())]);
    if let Some(marker) = marker {
        env.insert(MARKER.into(), marker.to_string_lossy().into_owned());
    }
    env
}

fn supervise(root: &Path, role: &str, timeout: &str) -> Result<Output> {
    Ok(cli(root)
        .env(ROLE, role)
        .env(MARKER, root.join("child.pid"))
        .args(["supervise", "--timeout", timeout, "--"])
        .args(child()?)
        .output()?)
}

// Re-enter this test binary as a finite-lived child without a shell or Python dependency.
#[test]
fn supervise_forwards_utf8_and_real_child_status() -> Result<()> {
    if let Ok(role) = std::env::var(ROLE) {
        let role = if role == "orphan" && std::env::args().any(|arg| arg == "--include-ignored") {
            "descendant"
        } else {
            role.as_str()
        };
        match role {
            "exit" => {
                println!("stdout: فارسی");
                eprintln!("stderr: پیام");
                stdout().flush()?;
                std::process::exit(37);
            }
            "flood" => {
                stdout().write_all(PRIVATE.repeat(3000).as_bytes())?;
            }
            "quiet" => thread::sleep(Duration::from_secs(4)),
            "progress" => {
                for _ in 0..40 {
                    println!("tick");
                    stdout().flush()?;
                    thread::sleep(Duration::from_millis(25));
                }
            }
            "env" => println!("cwd={} role={role}", std::env::current_dir()?.display()),
            "spawn" | "orphan" | "pipe-orphan" => {
                let argv = child()?;
                let marker = std::env::var(MARKER)?;
                // Spawn without inheritable capture handles on Windows, even if the CRT marks them inheritable.
                #[cfg(windows)]
                if role == "orphan" {
                    let line = format!("\"{}\" {} --include-ignored", argv[0], argv[1..].join(" "));
                    let _child = winsafe::CreateProcess(
                        Some(&argv[0]),
                        Some(&line),
                        None,
                        None,
                        false,
                        winsafe::co::CREATE::NO_WINDOW,
                        &[],
                        None,
                        &mut winsafe::STARTUPINFO::default(),
                    )?;
                    wait_for_marker(Path::new(&marker));
                    return Ok(());
                }
                let mut command = Command::new(&argv[0]);
                command
                    .args(&argv[1..])
                    .env(ROLE, "descendant")
                    .stdin(Stdio::null());
                if role == "orphan" {
                    command.stdout(Stdio::null()).stderr(Stdio::null());
                }
                let mut descendant = command.spawn()?;
                wait_for_marker(Path::new(&marker));
                if role == "spawn" {
                    assert!(descendant.wait()?.success());
                } else {
                    std::process::exit(0);
                }
            }
            "descendant" => {
                let marker = std::path::PathBuf::from(
                    std::env::var_os(MARKER).ok_or_else(|| anyhow!("missing marker"))?,
                );
                let lock = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&marker)?;
                lock.try_lock()?;
                fs::write(marker.with_extension("ready"), b"ready")?;
                thread::sleep(Duration::from_secs(8));
            }
            _ => return Err(anyhow!("unknown test child role")),
        }
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let output = supervise(scratch.path(), "exit", "15")?;
    assert_eq!(output.status.code(), Some(37));
    assert!(String::from_utf8(output.stdout)?.contains("stdout: فارسی"));
    assert!(String::from_utf8(output.stderr)?.contains("stderr: پیام"));
    Ok(())
}

fn assert_descendant_stopped(root: &Path) -> Result<()> {
    assert!(
        root.join("child.ready").is_file(),
        "descendant must run before cleanup is checked"
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("child.pid"))?;
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            Err(fs::TryLockError::WouldBlock) => assert!(
                start.elapsed() < Duration::from_secs(2),
                "owned descendant survived"
            ),
        }
        thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn wall_timeout_returns_124_and_terminates_descendant() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let start = Instant::now();
    let output = supervise(scratch.path(), "spawn", "2.5")?;
    assert_eq!(output.status.code(), Some(124));
    assert!(String::from_utf8(output.stderr)?.contains("wall deadline"));
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_descendant_stopped(scratch.path())
}

#[test]
fn exited_leader_with_pipe_holding_descendant_still_obeys_wall_deadline() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = supervise(scratch.path(), "pipe-orphan", "2.5")?;
    assert_eq!(output.status.code(), Some(124));
    assert_descendant_stopped(scratch.path())
}

#[test]
fn successful_leader_exit_still_terminates_silent_descendant() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = supervise(scratch.path(), "orphan", "4")?;
    assert_eq!(output.status.code(), Some(0));
    assert_descendant_stopped(scratch.path())
}

#[test]
fn owned_execution_uses_explicit_environment_and_working_directory() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let env = child_env("env", None);
    let output = run_owned(
        &child()?,
        &RunOptions {
            env: Some(&env),
            cwd: Some(scratch.path()),
            ..RunOptions::default()
        },
    )?;
    let out = String::from_utf8(output.stdout)?;
    assert!(out.contains("role=env"));
    assert!(out.contains(&scratch.path().display().to_string()));
    Ok(())
}

#[test]
fn exit_error_keeps_captured_bytes_without_displaying_them() -> Result<()> {
    let env = child_env("exit", None);
    let error = run_owned(
        &child()?,
        &RunOptions {
            env: Some(&env),
            ..RunOptions::default()
        },
    )
    .expect_err("child exit 37");
    assert_eq!(error.to_string(), "External tool failed (exit 37)");
    let RunError::Exit(output) = error else {
        panic!("missing captured exit data")
    };
    assert_eq!(exit_code(&output), 37);
    assert!(String::from_utf8(output.stdout)?.contains("stdout: فارسی"));
    Ok(())
}

#[test]
fn idle_deadline_is_distinct_and_check_false_does_not_disable_it() -> Result<()> {
    let env = child_env("quiet", None);
    let error = run_owned(
        &child()?,
        &RunOptions {
            env: Some(&env),
            timeout: Duration::from_secs(3),
            idle_timeout: Some(Duration::from_secs(1)),
            check: false,
            ..RunOptions::default()
        },
    )
    .expect_err("idle deadline");
    assert!(
        matches!(error, RunError::Timeout { wall: false, .. }),
        "{error}"
    );
    Ok(())
}

#[test]
fn timely_progress_does_not_extend_absolute_wall_deadline() -> Result<()> {
    let env = child_env("progress", None);
    let error = run_owned(
        &child()?,
        &RunOptions {
            env: Some(&env),
            timeout: Duration::from_millis(700),
            idle_timeout: Some(Duration::from_millis(400)),
            ..RunOptions::default()
        },
    )
    .expect_err("wall deadline");
    assert!(
        matches!(error, RunError::Timeout { wall: true, .. }),
        "{error}"
    );
    Ok(())
}

#[test]
fn owned_output_limit_refuses_flood_without_disclosing_body() -> Result<()> {
    let env = child_env("flood", None);
    let error = run_owned(
        &child()?,
        &RunOptions {
            env: Some(&env),
            max_output: 4096,
            ..RunOptions::default()
        },
    )
    .expect_err("output cap");
    let text = error.to_string();
    assert!(text.contains("output") && text.contains("limit"), "{text}");
    assert!(!text.contains(PRIVATE));
    Ok(())
}

#[test]
fn invalid_environment_is_rejected_before_executable_lookup() {
    let bad = BTreeMap::from([("BAD=NAME".into(), "value".into())]);
    let error = run_owned(
        &["nonexistent-test-executable".into()],
        &RunOptions {
            env: Some(&bad),
            ..RunOptions::default()
        },
    )
    .expect_err("invalid environment");
    assert!(matches!(error, RunError::Invalid(_)));
    assert!(error.to_string().contains("environment"));
}

#[test]
fn binary_nul_environment_is_rejected() {
    assert!(
        validate_environment(&BTreeMap::from([("NAME".into(), "value\0hidden".into())])).is_err()
    );
}

#[test]
fn missing_executable_is_reported_as_spawn_failure() {
    assert!(matches!(
        run_owned(
            &["no-such-revayat-tool-92745".into()],
            &RunOptions::default()
        ),
        Err(RunError::Spawn(_))
    ));
}

macro_rules! invalid_durations {
    ($($name:ident => $seconds:expr),+ $(,)?) => {$(
        #[test]
        fn $name() { assert!(duration($seconds, "test timeout").is_err()); }
    )+};
}
invalid_durations! { timeout_cannot_be_zero => 0.0, timeout_cannot_be_negative => -1.0,
timeout_cannot_be_infinite => f64::INFINITY, timeout_cannot_be_nan => f64::NAN,
timeout_cannot_exceed_one_day => 86_401.0 }

#[test]
fn fractional_timeout_retains_subsecond_precision() -> Result<()> {
    assert_eq!(duration(0.125, "timeout")?, Duration::from_millis(125));
    Ok(())
}

#[test]
fn json_round_trips_utf8_without_staging_residue() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("ترجمه.json");
    let data = json!({"text":"سلام\u{200c}دنیا","mixed":["Кино",13,false]});
    files::write_json(&path, &data).map_err(|e| anyhow!(e))?;
    assert_eq!(files::read_json(&path, 4096).map_err(|e| anyhow!(e))?, data);
    assert!(fs::read_to_string(path)?.contains("سلام\u{200c}دنیا"));
    assert_eq!(fs::read_dir(scratch.path())?.count(), 1);
    Ok(())
}

#[test]
fn json_replace_replaces_complete_old_bytes_without_appending() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("record.json");
    fs::write(&path, b"{\"old payload\":\"many bytes\"}\r\n")?;
    files::write_json(&path, &json!([1])).map_err(|e| anyhow!(e))?;
    assert_eq!(fs::read(path)?, b"[\n  1\n]\n");
    assert_eq!(fs::read_dir(scratch.path())?.count(), 1);
    Ok(())
}

#[test]
fn oversized_json_save_preserves_old_bytes_and_leaves_no_temp_file() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("record.json");
    let original = b"{\"keep\":true}\r\n";
    fs::write(&path, original)?;
    let error = files::write_json(&path, &json!("a".repeat(files::MAX_JSON_BYTES as usize)))
        .expect_err("JSON size budget");
    assert!(error.contains("byte limit"));
    assert_eq!(fs::read(path)?, original);
    assert_eq!(fs::read_dir(scratch.path())?.count(), 1);
    Ok(())
}

#[test]
fn failed_json_rename_preserves_competing_directory_and_reclaims_stage() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("record.json");
    fs::create_dir(&path)?;
    fs::write(path.join("foreign"), b"retain")?;
    assert!(files::write_json(&path, &json!({"new":true})).is_err());
    assert_eq!(fs::read(path.join("foreign"))?, b"retain");
    assert_eq!(fs::read_dir(scratch.path())?.count(), 1);
    Ok(())
}

#[test]
fn json_duplicate_keys_cannot_hide_an_earlier_value() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("record.json");
    fs::write(&path, b"{\"a\":1,\"a\":2}")?;
    assert!(files::read_json(&path, 4096).is_err());
    Ok(())
}

#[test]
fn log_filename_and_records_use_utc_and_close_cleanly() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let mut log = OperationalLog::in_directory("test-a", LogLevel::Debug, Some(scratch.path()))
        .map_err(|e| anyhow!(e))?;
    let path = log.path().expect("file logging").to_owned();
    assert!(
        path.file_name()
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.starts_with("test-a_") && p.ends_with("_UTC.log"))
    );
    log.record(LogLevel::Warning, "test warning");
    log.finish(2);
    let text = fs::read_to_string(&path)?;
    assert!(text.lines().all(|line| line.starts_with('[')
        && line.contains(" UTC] [")
        && line.contains("[test-a]")));
    assert!(text.contains("[WARNING] [test-a] test warning"));
    assert!(text.contains("exit_code=2"), "{text}");
    fs::remove_file(path)?;
    Ok(())
}

#[test]
fn same_directory_runs_use_distinct_non_overwritable_log_paths() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let open = || {
        OperationalLog::in_directory("same", LogLevel::Info, Some(scratch.path()))
            .map_err(|e| anyhow!(e))
    };
    let mut first = open()?;
    let mut second = open()?;
    let a = first.path().expect("first log").to_owned();
    let b = second.path().expect("second log").to_owned();
    assert_ne!(a, b);
    first.record(LogLevel::Info, "run one");
    second.record(LogLevel::Info, "run two");
    first.finish(0);
    second.finish(0);
    assert!(fs::read_to_string(a)?.contains("run one"));
    assert!(fs::read_to_string(b)?.contains("run two"));
    Ok(())
}

#[test]
fn debug_log_level_retains_debug_and_info_filters_it() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    for (level, expected) in [(LogLevel::Debug, true), (LogLevel::Info, false)] {
        let mut log = OperationalLog::in_directory("debug-test", level, Some(scratch.path()))
            .map_err(|e| anyhow!(e))?;
        let path = log.path().expect("file").to_owned();
        log.record(LogLevel::Debug, "debug marker");
        log.finish(0);
        assert_eq!(fs::read_to_string(path)?.contains("debug marker"), expected);
    }
    Ok(())
}

#[test]
fn log_destination_as_file_falls_back_to_formatted_stderr() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(scratch.path().join("logs"), b"keep")?;
    let output = cli(scratch.path())
        .args(["status", "--work", "missing"])
        .output()?;
    let error = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert!(error.contains("UTC] [WARNING] [revayat-subtitle] File logging unavailable"));
    assert_eq!(fs::read(scratch.path().join("logs"))?, b"keep");
    Ok(())
}

#[test]
fn invalid_log_setting_does_not_echo_value_or_create_a_log() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let output = cli(scratch.path())
        .env("REVAYAT_LOG_LEVEL", "private-invalid-option")
        .args(["status", "--work", "missing"])
        .output()?;
    let error = String::from_utf8(output.stderr)?;
    assert_eq!(output.status.code(), Some(2));
    assert!(error.contains("Log level must be"));
    assert!(!error.contains("private-invalid-option"));
    assert!(!scratch.path().join("logs").exists());
    Ok(())
}

#[test]
fn malformed_source_subtitle_body_never_reaches_operational_diagnostics() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    fs::write(
        scratch.path().join("source.srt"),
        format!("1\nnot the timestamp\n{PRIVATE}\n"),
    )?;
    let output = cli(scratch.path())
        .args(["convert", "source.srt", "--out", "translated.ass"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8(output.stderr)?.contains(PRIVATE));
    let log = fs::read_dir(scratch.path().join("logs"))?
        .next()
        .expect("run log")?;
    assert!(!fs::read_to_string(log.path())?.contains(PRIVATE));
    assert!(!scratch.path().join("translated.ass").exists());
    Ok(())
}

macro_rules! redaction {
    ($($name:ident => ($input:literal, $hidden:literal)),+ $(,)?) => {$(
        #[test]
        fn $name() {
            let cleaned = diagnostic::redact($input);
            assert!(!cleaned.contains($hidden), "{cleaned}");
            assert!(cleaned.contains("[REDACTED]"));
        }
    )+};
}
redaction! {
    password_is_redacted => ("password='fake password'", "fake password"),
    cookie_is_redacted => ("Cookie: sid=fictional-cookie", "fictional-cookie"),
    header_token_is_redacted => ("Authorization: Bearer pretend.jwt", "pretend.jwt"),
    url_userinfo_is_redacted => ("https://demo:unreal@example.invalid/path", "demo:unreal"),
    api_key_is_redacted => ("api_key=synthetic-example", "synthetic-example"),
}
