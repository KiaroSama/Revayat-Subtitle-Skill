use process_wrap::std::{ChildWrapper, CommandWrap};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const WALL_MAX: Duration = Duration::from_secs(86_400);
const MAX_OUTPUT: usize = 256 * 1024 * 1024;
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Request cooperative cancellation without abandoning owned child processes.
/// Executables install their own signal handler; embedded users retain control.
pub fn request_cancel() {
    CANCEL_REQUESTED.store(true, Ordering::Release);
}

pub fn cancel_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::Acquire)
}

#[derive(Debug)]
pub enum RunError {
    Cancelled,
    Invalid(&'static str),
    Spawn(io::Error),
    Io(io::Error),
    Timeout { wall: bool, elapsed: Duration },
    Exit(Output),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("External tool cancelled; owned processes were stopped"),
            Self::Invalid(s) => f.write_str(s),
            Self::Spawn(e) | Self::Io(e) => e.fmt(f),
            Self::Timeout { wall, elapsed } => write!(
                f,
                "External tool {} deadline expired after {:.3} seconds",
                if *wall { "wall" } else { "idle" },
                elapsed.as_secs_f64()
            ),
            Self::Exit(out) => write!(f, "External tool failed (exit {})", exit_code(out)),
        }
    }
}
impl std::error::Error for RunError {}

pub fn exit_code(output: &Output) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        output
            .status
            .code()
            .or_else(|| output.status.signal().map(|v| -v))
            .unwrap_or(-1)
    }
    #[cfg(not(unix))]
    {
        output.status.code().unwrap_or(-1)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions<'a> {
    pub cwd: Option<&'a Path>,
    pub timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_output: usize,
    pub env: Option<&'a BTreeMap<String, String>>,
    pub check: bool,
}
impl Default for RunOptions<'_> {
    fn default() -> Self {
        Self {
            cwd: None,
            timeout: Duration::from_secs(45),
            idle_timeout: None,
            max_output: 16 * 1024 * 1024,
            env: None,
            check: true,
        }
    }
}

pub fn duration(secs: f64, name: &'static str) -> Result<Duration, RunError> {
    Duration::try_from_secs_f64(secs)
        .ok()
        .filter(|d| secs > 0.0 && *d <= WALL_MAX)
        .map(|d| d.max(Duration::from_nanos(1)))
        .ok_or(RunError::Invalid(name))
}

fn string_size(value: &str) -> usize {
    2 + value
        .chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' | '"' | '\\' => 2,
            ' '..='~' => 1,
            _ => 6 * c.len_utf16(),
        })
        .sum::<usize>()
}

pub fn validate_environment(env: &BTreeMap<String, String>) -> Result<(), RunError> {
    if env.len() > 1000 {
        return Err(RunError::Invalid(
            "Tool environment must be a bounded string mapping",
        ));
    }
    let mut size = 2;
    for (key, value) in env {
        if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
            return Err(RunError::Invalid(
                "Tool environment contains an invalid key or value",
            ));
        }
        size += string_size(key) + string_size(value) + 4;
    }
    if size.saturating_sub(2) > 1024 * 1024 {
        return Err(RunError::Invalid(
            "Tool environment exceeds its byte budget",
        ));
    }
    Ok(())
}

struct Progress {
    latest: Instant,
    expired: bool,
}
enum Event {
    Bytes(usize, Vec<u8>),
    Eof,
    Error(io::Error),
}

fn reader(
    mut pipe: impl Read + Send + 'static,
    id: usize,
    tx: mpsc::SyncSender<Event>,
    progress: Arc<Mutex<Progress>>,
    idle: Duration,
) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("tool-capture-{id}"))
        .spawn(move || {
            let mut buf = vec![0; 65_536];
            loop {
                let event = match pipe.read(&mut buf) {
                    Ok(0) => Event::Eof,
                    Ok(n) => {
                        let mut observed = progress
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let now = Instant::now();
                        observed.expired |= now.duration_since(observed.latest) >= idle;
                        observed.latest = now;
                        Event::Bytes(id, buf[..n].to_vec())
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => Event::Error(e),
                };
                let end = !matches!(event, Event::Bytes(..));
                if tx.send(event).is_err() || end {
                    return;
                }
            }
        })
}

fn leader_finished(child: &mut dyn ChildWrapper) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
        let pid = i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| io::Error::other("Invalid owned child PID"))?;
        // Keep the leader unreaped until after group termination, so its PGID cannot be reused.
        waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map(|status| status.is_some())
        .map_err(Into::into)
    }
    #[cfg(windows)]
    {
        child.inner_mut().try_wait().map(|status| status.is_some())
    }
}

fn finish(
    child: &mut dyn ChildWrapper,
    readers: Vec<thread::JoinHandle<()>>,
) -> io::Result<std::process::ExitStatus> {
    let kill = child.start_kill();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Owned tool did not terminate within cleanup budget",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    };
    #[cfg(unix)]
    if let Err(e) = kill
        && e.raw_os_error() != Some(3)
    {
        return Err(e);
    }
    #[cfg(not(unix))]
    kill?;
    for reader in readers {
        while !reader.is_finished() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Tool capture did not finish after cleanup",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
        reader
            .join()
            .map_err(|_| io::Error::other("Tool capture thread failed"))?;
    }
    Ok(status)
}

pub fn run_owned(command: &[String], options: &RunOptions<'_>) -> Result<Output, RunError> {
    if cancel_requested() {
        return Err(RunError::Cancelled);
    }
    let idle = options.idle_timeout.unwrap_or(options.timeout);
    for (duration, message) in [
        (options.timeout, "timeout must be a finite positive number"),
        (idle, "idle_timeout must be a finite positive number"),
    ] {
        if duration.is_zero() || duration > WALL_MAX {
            return Err(RunError::Invalid(message));
        }
    }
    if !(1..=MAX_OUTPUT).contains(&options.max_output) {
        return Err(RunError::Invalid(
            "max_output must be an integer from 1 to 268435456 bytes",
        ));
    }
    let (program, args) = command
        .split_first()
        .filter(|(program, _)| !program.is_empty() && command.iter().all(|v| !v.contains('\0')))
        .ok_or(RunError::Invalid(
            "Tool command must be a nonempty argv list of strings",
        ))?;
    if let Some(env) = options.env {
        validate_environment(env)?;
    }
    if command
        .iter()
        .map(|arg| string_size(arg) + 2)
        .sum::<usize>()
        > 65_535
    {
        return Err(RunError::Invalid(
            "Tool command exceeds the argument budget",
        ));
    }
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = options.cwd {
        cmd.current_dir(dir);
    }
    if let Some(env) = options.env {
        cmd.env_clear().envs(env);
    }
    let mut wrapped = CommandWrap::from(cmd);
    #[cfg(windows)]
    wrapped
        .wrap(process_wrap::std::CreationFlags(0x0800_0000))
        .wrap(process_wrap::std::KillOnDrop)
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::std::ProcessGroup::leader());
    let began = Instant::now();
    let mut child = wrapped.spawn().map_err(RunError::Spawn)?;
    let (tx, rx) = mpsc::sync_channel(16);
    let progress = Arc::new(Mutex::new(Progress {
        latest: began,
        expired: false,
    }));
    let mut readers = Vec::with_capacity(2);
    let mut buffers = [Vec::new(), Vec::new()];
    let result = (|| {
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| io::Error::other("Missing tool stdout"))?;
        let stderr = child
            .stderr()
            .take()
            .ok_or_else(|| io::Error::other("Missing tool stderr"))?;
        readers.push(reader(stdout, 0, tx.clone(), Arc::clone(&progress), idle)?);
        readers.push(reader(stderr, 1, tx, Arc::clone(&progress), idle)?);
        let mut eof = 0;
        let mut size = 0;
        loop {
            if cancel_requested() {
                return Ok(Some(RunError::Cancelled));
            }
            let progress = progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let now = Instant::now();
            let elapsed = now.duration_since(began);
            let wall_left = options.timeout.saturating_sub(elapsed);
            let idle_left = idle.saturating_sub(now.duration_since(progress.latest));
            let remaining = wall_left.min(idle_left);
            if remaining.is_zero() || progress.expired {
                return Ok(Some(RunError::Timeout {
                    wall: wall_left.is_zero(),
                    elapsed,
                }));
            }
            drop(progress);
            if eof == 2 {
                if leader_finished(&mut *child)? {
                    return Ok(None);
                }
                thread::sleep(remaining.min(Duration::from_millis(5)));
                continue;
            }
            match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(Event::Bytes(id, bytes)) => {
                    size += bytes.len();
                    if size > options.max_output {
                        return Ok(Some(RunError::Invalid(
                            "External tool output exceeded its byte limit",
                        )));
                    }
                    buffers[id].extend(bytes);
                }
                Ok(Event::Eof) => eof += 1,
                Ok(Event::Error(e)) => return Err(e),
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("Tool capture disconnected"));
                }
            }
        }
    })();
    drop(rx);
    let status = finish(&mut *child, readers).map_err(RunError::Io)?;
    if cancel_requested() {
        return Err(RunError::Cancelled);
    }
    if let Some(error) = result.map_err(RunError::Io)? {
        return Err(error);
    }
    let [stdout, stderr] = buffers;
    let output = Output {
        status,
        stdout,
        stderr,
    };
    if options.check && !output.status.success() {
        return Err(RunError::Exit(output));
    }
    Ok(output)
}

fn environment(v: &Value) -> Result<Option<BTreeMap<String, String>>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let map = v
        .as_object()
        .filter(|m| m.len() <= 1000)
        .ok_or("ValueError: Tool environment must be a bounded string mapping")?;
    let contents = map
        .iter()
        .map(|(k, v)| {
            v.as_str()
                .map(|v| (k.clone(), v.to_owned()))
                .ok_or("ValueError: Tool environment contains an invalid key or value")
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    validate_environment(&contents).map_err(|e| format!("ValueError: {e}"))?;
    Ok(Some(contents))
}

pub fn checked_environment(env: Value) -> Result<Value, String> {
    environment(&env)?;
    Ok(env)
}

pub fn run(
    command: Vec<String>,
    cwd: Option<Value>,
    timeout: f64,
    idle_timeout: Option<f64>,
    max_output: Option<i64>,
    env: Option<Value>,
    check: bool,
) -> Result<Value, String> {
    let positive = |seconds, name| duration(seconds, name).map_err(|e| format!("ValueError: {e}"));
    let wall = positive(timeout, "timeout must be a finite positive number")?;
    let idle_timeout = idle_timeout
        .map(|seconds| positive(seconds, "idle_timeout must be a finite positive number"))
        .transpose()?;
    let max_output = max_output.unwrap_or(16 * 1024 * 1024);
    if !(1..=256 * 1024 * 1024).contains(&max_output) {
        return Err("ValueError: max_output must be an integer from 1 to 268435456 bytes".into());
    }
    if command.first().is_none_or(String::is_empty) || command.iter().any(|v| v.contains('\0')) {
        return Err("ValueError: Tool command must be a nonempty argv list of strings".into());
    }
    let env = env.as_ref().map(environment).transpose()?.flatten();
    let cwd = cwd
        .as_ref()
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .or_else(|| v["$path"].as_str())
                .map(Path::new)
                .ok_or("TypeError: expected str, bytes or os.PathLike object")
        })
        .transpose()?;
    let options = RunOptions {
        cwd,
        timeout: wall,
        idle_timeout,
        max_output: max_output as usize,
        env: env.as_ref(),
        check,
    };
    let output = match run_owned(&command, &options) {
        Ok(output) => output,
        Err(RunError::Cancelled) => {
            return Err("process_control.Cancelled: External tool cancelled".into());
        }
        Err(RunError::Spawn(e)) if cfg!(windows) => {
            if e.raw_os_error() == Some(267) || cwd.is_some_and(|p| !p.is_dir()) {
                return Err(format!("NotADirectoryError: {e}"));
            }
            let diagnostic = b"ERROR: Supervised target could not start.\r\n";
            if diagnostic.len() > options.max_output {
                return Err("ValueError: External tool output exceeded its byte limit".into());
            }
            if !check {
                return Ok(completed(command, 2, Vec::new(), diagnostic.to_vec()));
            }
            return Err("process_control.ToolFailure: External tool failed (exit 2)".into());
        }
        Err(RunError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("FileNotFoundError: {e}"));
        }
        Err(RunError::Spawn(e) | RunError::Io(e)) => return Err(format!("OSError: {e}")),
        Err(RunError::Invalid(message)) => return Err(format!("ValueError: {message}")),
        Err(error @ RunError::Timeout { .. }) => {
            return Err(format!("subprocess.TimeoutExpired: {error}"));
        }
        Err(error @ RunError::Exit(_)) => {
            return Err(format!("process_control.ToolFailure: {error}"));
        }
    };
    if check {
        Ok(json!({"$bytes":output.stdout}))
    } else {
        Ok(completed(
            command,
            exit_code(&output),
            output.stdout,
            output.stderr,
        ))
    }
}

fn completed(args: Vec<String>, returncode: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> Value {
    json!({"$object":"CompletedProcess","attrs":{"args":args,"returncode":returncode,"stdout":{"$bytes":stdout},"stderr":{"$bytes":stderr}}})
}

#[cfg(test)]
#[path = "process_control_tests.rs"]
mod tests;
