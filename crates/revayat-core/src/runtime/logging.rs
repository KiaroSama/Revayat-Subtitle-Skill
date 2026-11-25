use super::diagnostic::line;
use std::{
    env, fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

pub(crate) fn stderr_line(line: &str) {
    let _ = writeln!(io::stderr(), "{line}");
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl std::str::FromStr for LogLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARNING" => Ok(Self::Warning),
            "ERROR" => Ok(Self::Error),
            "CRITICAL" => Ok(Self::Critical),
            _ => Err("Log level must be DEBUG, INFO, WARNING, ERROR or CRITICAL".into()),
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        })
    }
}

pub struct OperationalLog {
    component: String,
    level: LogLevel,
    started: Instant,
    file: Option<(PathBuf, File)>,
}

impl OperationalLog {
    pub fn start(component: &str, level: Option<LogLevel>) -> Result<Self, String> {
        let level = match (level, env::var("REVAYAT_LOG_LEVEL")) {
            (Some(level), _) => level,
            (None, Ok(level)) => level.parse()?,
            (None, Err(env::VarError::NotPresent)) => LogLevel::Info,
            (None, Err(error)) => return Err(format!("Invalid logging configuration: {error}")),
        };
        let directory = env::var_os("REVAYAT_LOG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(|parent| parent.join("logs")))
            });
        Self::in_directory(component, level, directory.as_deref())
    }

    pub fn in_directory(
        component: &str,
        level: LogLevel,
        directory: Option<&Path>,
    ) -> Result<Self, String> {
        if component.is_empty()
            || component.len() > 64
            || !component
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        {
            return Err("Invalid log component".into());
        }
        let file = directory.and_then(|path| open(path, component).ok());
        if file.is_none() {
            stderr_line(&line(
                component,
                "WARNING",
                "File logging unavailable; using stderr.",
            ));
        }
        let mut log = Self {
            component: component.into(),
            level,
            started: Instant::now(),
            file,
        };
        log.record(
            LogLevel::Info,
            &format!(
                "Started version={} pid={}",
                env!("CARGO_PKG_VERSION"),
                std::process::id()
            ),
        );
        Ok(log)
    }

    pub fn path(&self) -> Option<&Path> {
        self.file.as_ref().map(|(path, _)| path.as_path())
    }

    pub fn record(&mut self, level: LogLevel, message: &str) {
        if level < self.level {
            return;
        }
        self.write_line(level, message);
    }

    fn write_line(&mut self, level: LogLevel, message: &str) {
        let entry = line(&self.component, &level.to_string(), message);
        if let Some((_, file)) = &mut self.file {
            if writeln!(file, "{entry}")
                .and_then(|()| file.flush())
                .is_ok()
            {
                if level >= LogLevel::Error {
                    stderr_line(&entry);
                }
                return;
            }
            self.file = None;
            stderr_line(&line(
                &self.component,
                "WARNING",
                "File logging failed; using stderr.",
            ));
        }
        stderr_line(&entry);
    }

    pub fn finish(mut self, exit_code: i32) {
        let level = if exit_code > 1 {
            LogLevel::Error
        } else {
            LogLevel::Info
        };
        let elapsed = self.started.elapsed().as_secs_f64();
        self.record(
            level,
            &format!("Finished exit_code={exit_code} elapsed_seconds={elapsed:.3}"),
        );
    }
}

fn open(directory: &Path, component: &str) -> io::Result<(PathBuf, File)> {
    fs::create_dir_all(directory)?;
    let stamp = jiff::Timestamp::now().strftime("%Y-%m-%d_%H-%M-%S_UTC");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    for i in 0..1000 {
        let suffix = if i == 0 {
            String::new()
        } else {
            format!("_{i}")
        };
        let path = directory.join(format!("{component}_{stamp}{suffix}.log"));
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Log filename collision limit reached",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_distinct_utc_logs_and_redacts_credentials() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut first = OperationalLog::in_directory("test", LogLevel::Debug, Some(dir.path()))?;
        let second = OperationalLog::in_directory("test", LogLevel::Info, Some(dir.path()))?;
        assert_ne!(first.path(), second.path());
        let path = first.path().ok_or("expected a log file")?.to_path_buf();
        assert!(path.to_string_lossy().contains("_UTC"));
        first.record(LogLevel::Debug, "token=fixture-value \u{0627}\u{0628}");
        first.finish(0);
        let text = fs::read_to_string(path)?;
        assert!(text.contains("[DEBUG] [test] token=[REDACTED] \u{0627}\u{0628}"));
        assert!(text.contains("Finished exit_code=0"));
        assert!(!text.contains("fixture-value"));
        Ok(())
    }
}
