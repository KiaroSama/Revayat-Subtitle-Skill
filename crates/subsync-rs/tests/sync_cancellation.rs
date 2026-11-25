#![cfg(target_os = "linux")]

mod support;

use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use std::{
    fs,
    io::{Read, Seek},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use support::{Fixture, report, run};

struct OwnedChild {
    child: Child,
    fifo: PathBuf,
    observed: Vec<(i32, Vec<u8>)>,
}

impl OwnedChild {
    fn observe(&mut self) {
        let Ok(processes) = fs::read_dir("/proc") else {
            return;
        };
        for entry in processes.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|pid| pid.parse::<i32>().ok())
            else {
                continue;
            };
            if self.observed.iter().any(|(known, _)| *known == pid) {
                continue;
            }
            let is_child = fs::read_to_string(entry.path().join("stat")).is_ok_and(|stat| {
                stat.rsplit_once(") ")
                    .and_then(|(_, tail)| tail.split_whitespace().nth(1))
                    .and_then(|parent| parent.parse::<u32>().ok())
                    == Some(self.child.id())
            });
            if !is_child {
                continue;
            }
            if let Ok(command) = fs::read(format!("/proc/{pid}/cmdline"))
                && command
                    .split(|byte| *byte == 0)
                    .any(|arg| arg == self.fifo.as_os_str().as_bytes())
            {
                self.observed.push((pid, command));
            }
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.observe();
        for (pid, command) in &self.observed {
            if fs::read(format!("/proc/{pid}/cmdline")).ok().as_ref() == Some(command) {
                let _ = killpg(Pid::from_raw(*pid), Signal::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn active(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(") ")
            .is_some_and(|(_, tail)| !tail.starts_with(['Z', 'X']))
    })
}

#[test]
fn interrupt_and_termination_clean_up_real_ffmpeg_before_exiting_130() {
    let positive = Fixture::new();
    positive.write_subtitle("\n");
    assert_eq!(report(run(positive.command()))["cues_total"], 4);
    for signal in [Signal::SIGINT, Signal::SIGTERM] {
        let fixture = Fixture::new();
        fixture.write_subtitle("\n");
        let fifo = fixture.directory.path().join("blocked authored media.mkv");
        let mut mkfifo = Command::new("mkfifo");
        mkfifo.arg(&fifo);
        assert!(run(mkfifo).status.success());
        let mut diagnostic = tempfile::tempfile().unwrap();
        let child = fixture
            .command_for("--video", &fifo)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(diagnostic.try_clone().unwrap())
            .spawn()
            .unwrap();
        let mut owner = OwnedChild {
            child,
            fifo,
            observed: Vec::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while owner.observed.is_empty() {
            owner.observe();
            assert!(
                owner.child.try_wait().unwrap().is_none(),
                "CLI exited before FFmpeg started"
            );
            assert!(Instant::now() < deadline, "No FFmpeg process observed");
            thread::sleep(Duration::from_millis(10));
        }
        kill(Pid::from_raw(owner.child.id() as i32), signal).unwrap();
        let deadline = Instant::now() + Duration::from_secs(7);
        let status = loop {
            if let Some(status) = owner.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "Cancellation did not finish within its cleanup budget"
            );
            thread::sleep(Duration::from_millis(10));
        };
        diagnostic.rewind().unwrap();
        let mut text = String::new();
        diagnostic.read_to_string(&mut text).unwrap();
        assert_eq!(status.code(), Some(130), "{signal:?}: {text}");
        assert!(text.contains("cancelled"), "{text}");
        assert!(!text.contains("cleanup failed"), "{text}");
        assert!(
            owner.observed.iter().all(|(pid, _)| !active(*pid)),
            "FFmpeg survived cancellation"
        );
        assert!(!fixture.output.exists());
        assert!(!fixture.receipt.exists());
    }
}
