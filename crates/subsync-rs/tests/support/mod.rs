use serde_json::Value;
use std::{
    f64::consts::TAU,
    fs,
    io::{Read, Seek},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::{TempDir, tempfile};

pub const INTERVALS: [(u64, u64); 4] = [(350, 970), (1590, 2630), (3710, 4480), (5110, 6640)];
pub const CAPTIONS: &str = "<i>سلام</i> و <b>دنیا</b>";

pub struct Fixture {
    pub directory: TempDir,
    pub wav: PathBuf,
    pub subtitle: PathBuf,
    pub output: PathBuf,
    pub receipt: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let wav = directory.path().join("sample audio.wav");
        let subtitle = directory.path().join("original.srt");
        let output = directory.path().join("aligned.srt");
        let receipt = directory.path().join("report.json");
        let samples = (0..8 * 16_000)
            .map(|sample| {
                if INTERVALS
                    .iter()
                    .any(|&(begin, end)| (begin * 16..end * 16).contains(&sample))
                {
                    tone(sample)
                } else {
                    ((sample * 4093 % 131) as i16) - 65
                }
            })
            .collect::<Vec<_>>();
        subsync_rs::audio::write_wav_pcm16(&wav, 16_000, &samples).unwrap();
        Self {
            directory,
            wav,
            subtitle,
            output,
            receipt,
        }
    }

    pub fn srt(&self, shift: u64, newline: &str) -> String {
        INTERVALS
            .iter()
            .enumerate()
            .map(|(index, &(begin, end))| {
                let separator = if index + 1 == INTERVALS.len() {
                    ""
                } else {
                    newline
                };
                format!(
                    "{}{newline}{} --> {}{newline}{CAPTIONS}{newline}{separator}",
                    index + 1,
                    timestamp(begin + shift),
                    timestamp(end + shift)
                )
            })
            .collect()
    }

    pub fn write_subtitle(&self, newline: &str) {
        fs::write(&self.subtitle, self.srt(1300, newline)).unwrap();
    }

    pub fn command_for(&self, flag: &str, media: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_subsync"));
        command
            .current_dir(self.directory.path())
            .arg(flag)
            .arg(media)
            .arg("--in")
            .arg(&self.subtitle)
            .arg("--out")
            .arg(&self.output)
            .arg("--json-report")
            .arg(&self.receipt)
            .arg("--no-fps-search");
        command
    }

    pub fn command(&self) -> Command {
        self.command_for("--wav", &self.wav)
    }
}

pub fn tone(sample: u64) -> i16 {
    [400.0, 1200.0, 2400.0]
        .iter()
        .map(|frequency| 3500.0 * (TAU * frequency * sample as f64 / 16_000.0).sin())
        .sum::<f64>() as i16
}

fn timestamp(ms: u64) -> String {
    format!("00:00:{:02},{:03}", ms / 1000, ms % 1000)
}

pub fn run(mut command: Command) -> Output {
    let (mut stdout, mut stderr) = (tempfile().unwrap(), tempfile().unwrap());
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap());
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("could not run {command:?}: {error}"));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("test command did not exit: {command:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut captured = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    stdout.rewind().unwrap();
    stderr.rewind().unwrap();
    stdout.read_to_end(&mut captured.stdout).unwrap();
    stderr.read_to_end(&mut captured.stderr).unwrap();
    captured
}

pub fn report(result: Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
