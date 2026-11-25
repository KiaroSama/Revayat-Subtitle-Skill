#![cfg(target_os = "linux")]

use std::{
    fs::OpenOptions,
    process::{Command, Stdio},
};

#[test]
fn fallback_diagnostic_never_panics_when_stderr_is_unwritable()
-> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "REVAYAT_FALLBACK_LOG_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        assert!(
            revayat_core::runtime::log_fallback(
                "authored-fixture".into(),
                "ERROR".into(),
                "Controlled failure".into()
            )?
            .is_null()
        );
        return Ok(());
    }
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "fallback_diagnostic_never_panics_when_stderr_is_unwritable",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(OpenOptions::new().write(true).open("/dev/full")?)
        .status()?;
    assert!(output.success(), "fallback logging returned {output}");
    Ok(())
}
