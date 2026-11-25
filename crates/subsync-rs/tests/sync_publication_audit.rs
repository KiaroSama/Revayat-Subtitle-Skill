mod support;

use std::{fs, process::Command};
use support::{Fixture, report, run};

fn collision(target: &str) {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let source_before = fs::read(&fixture.subtitle).unwrap();
    let audio_before = fs::read(&fixture.wav).unwrap();
    let mut command = fixture.command();
    match target {
        "input" => {
            command.arg("--out").arg(&fixture.subtitle);
        }
        "audio" => {
            command.arg("--out").arg(&fixture.wav);
        }
        "report-input" => {
            command.arg("--json-report").arg(&fixture.subtitle);
        }
        "report-audio" => {
            command.arg("--json-report").arg(&fixture.wav);
        }
        "output-report" => {
            command.arg("--json-report").arg(&fixture.output);
        }
        _ => unreachable!(),
    }
    assert!(!run(command).status.success());
    assert_eq!(fs::read(&fixture.subtitle).unwrap(), source_before);
    assert_eq!(fs::read(&fixture.wav).unwrap(), audio_before);
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[test]
fn rejects_output_alias_of_subtitle() {
    collision("input");
}
#[test]
fn rejects_output_alias_of_wav() {
    collision("audio");
}
#[test]
fn rejects_report_alias_of_subtitle() {
    collision("report-input");
}
#[test]
fn rejects_report_alias_of_wav() {
    collision("report-audio");
}
#[test]
fn rejects_output_and_report_alias() {
    collision("output-report");
}

fn hardlink(is_report: bool) {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let original = fs::read(&fixture.subtitle).unwrap();
    let alias = if is_report {
        &fixture.receipt
    } else {
        &fixture.output
    };
    fs::hard_link(&fixture.subtitle, alias).unwrap();
    assert!(!run(fixture.command()).status.success());
    assert_eq!(fs::read(&fixture.subtitle).unwrap(), original);
    assert_eq!(fs::read(alias).unwrap(), original);
    assert_eq!(fixture.output.exists(), !is_report);
    assert_eq!(fixture.receipt.exists(), is_report);
}

#[test]
fn refuses_hardlinked_subtitle_output() {
    hardlink(false);
}
#[test]
fn refuses_hardlinked_report_output() {
    hardlink(true);
}

#[cfg(unix)]
#[test]
fn refuses_output_report_alias_through_symlinked_parent() {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let link = fixture.directory.path().join("linked parent");
    std::os::unix::fs::symlink(fixture.directory.path(), &link).unwrap();
    let mut command = fixture.command();
    command
        .arg("--json-report")
        .arg(link.join(fixture.output.file_name().unwrap()));
    let result = run(command);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("different files"));
    assert!(!fixture.output.exists());
}

#[cfg(unix)]
fn dangling_symlink(is_report: bool) {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let alias = if is_report {
        &fixture.receipt
    } else {
        &fixture.output
    };
    let missing = fixture.directory.path().join("absent");
    std::os::unix::fs::symlink(&missing, alias).unwrap();
    assert!(!run(fixture.command()).status.success());
    assert_eq!(fs::read_link(alias).unwrap(), missing);
    assert!(!missing.exists());
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[cfg(unix)]
#[test]
fn preserves_dangling_subtitle_output_symlink() {
    dangling_symlink(false);
}
#[cfg(unix)]
#[test]
fn preserves_dangling_report_output_symlink() {
    dangling_symlink(true);
}

fn failed_publication(is_report: bool) {
    let mut fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let blocked = fixture.directory.path().join("output parent");
    fs::write(&blocked, b"do not replace").unwrap();
    let final_path = if is_report {
        &mut fixture.receipt
    } else {
        &mut fixture.output
    };
    let normal_path = std::mem::replace(final_path, blocked.join(final_path.file_name().unwrap()));
    let result = run(fixture.command());
    assert!(!result.status.success());
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
    assert_eq!(fs::read(&blocked).unwrap(), b"do not replace");
    *if is_report {
        &mut fixture.receipt
    } else {
        &mut fixture.output
    } = normal_path;
    assert!(report(run(fixture.command()))["cues_total"].is_number());
    assert!(fixture.output.is_file() && fixture.receipt.is_file());
}

#[test]
fn failed_subtitle_write_can_retry_without_partial_output() {
    failed_publication(false);
}
#[test]
fn failed_report_write_leaves_no_subtitle_and_can_retry() {
    failed_publication(true);
}

#[cfg(target_os = "linux")]
#[test]
fn kernel_partial_write_leaves_output_name_retryable() {
    let fixture = Fixture::new();
    let subtitle = fixture
        .srt(1300, "\n")
        .replace(support::CAPTIONS, &"large subtitle ".repeat(1200));
    fs::write(&fixture.subtitle, &subtitle).unwrap();
    let entries = || {
        fs::read_dir(fixture.directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = entries();
    let binary = fixture.command();
    let mut limited = Command::new("env");
    limited
        .args(["--ignore-signal=XFSZ", "prlimit", "--fsize=128:128", "--"])
        .arg(binary.get_program())
        .args(binary.get_args());
    let rejected = run(limited);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("complete staged output"),
        "{rejected:?}"
    );
    assert_eq!(entries(), before);
    assert_eq!(
        report(run(fixture.command()))["cues_total"],
        support::INTERVALS.len()
    );
    assert!(
        fs::read_to_string(&fixture.output)
            .unwrap()
            .contains(&"large subtitle ".repeat(1200))
    );
}

#[cfg(target_os = "linux")]
#[test]
fn readonly_report_directory_leaves_no_subtitle_and_can_retry() {
    use std::os::unix::fs::PermissionsExt;

    let mut fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let reports = fixture.directory.path().join("read only reports");
    fs::create_dir(&reports).unwrap();
    fixture.receipt = reports.join("report.json");
    fs::set_permissions(&reports, fs::Permissions::from_mode(0o500)).unwrap();
    let rejected = run(fixture.command());
    fs::set_permissions(&reports, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        !rejected.status.success(),
        "Run this test as a non-root user: {rejected:?}"
    );
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
    let receipt = report(run(fixture.command()));
    assert_eq!(
        receipt,
        serde_json::from_slice::<serde_json::Value>(&fs::read(&fixture.receipt).unwrap()).unwrap()
    );
}

#[test]
fn uses_packaged_bin_when_called_by_absolute_path() {
    let fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let name = format!("subsync{}", std::env::consts::EXE_SUFFIX);
    let root_bin = fixture.directory.path().join("bin");
    let competitor = fixture.directory.path().join("path");
    for path in [&root_bin, &competitor] {
        fs::create_dir(path).unwrap();
    }
    let exe = root_bin.join(&name);
    fs::copy(env!("CARGO_BIN_EXE_subsync"), &exe).unwrap();
    fs::write(competitor.join(name), []).unwrap();
    let default = fixture.command();
    let mut command = Command::new(&exe);
    command
        .args(default.get_args())
        .env("PATH", competitor)
        .current_dir(fixture.directory.path());
    assert!(report(run(command))["cues_total"].is_number());
}

#[test]
fn silence_and_existing_output_are_safe_failures() {
    let mut fixture = Fixture::new();
    fixture.write_subtitle("\n");
    let original = fs::read(&fixture.subtitle).unwrap();
    fixture.wav = fixture.directory.path().join("silent.wav");
    subsync_rs::audio::write_wav_pcm16(&fixture.wav, 16_000, &[0; 16_000]).unwrap();
    let result = run(fixture.command());
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("No usable speech"));
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
    fs::write(&fixture.output, b"existing deliverable").unwrap();
    let result = run(fixture.command());
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("existing files"));
    assert_eq!(fs::read(&fixture.output).unwrap(), b"existing deliverable");
    assert_eq!(fs::read(&fixture.subtitle).unwrap(), original);
    assert!(!fixture.receipt.exists());
}
