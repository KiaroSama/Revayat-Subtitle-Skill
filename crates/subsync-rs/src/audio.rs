use crate::{SyncError, check_cancelled, publication::stage_file};
use process_wrap::std::{ChildWrapper, CommandWrap};
use std::{
    fs::{self, File},
    io::{self, BufWriter, Read, Seek, Write},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const VIDEO_SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_FFMPEG_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_FFMPEG_TIMEOUT: Duration = Duration::from_secs(86_400);
const POLL: Duration = Duration::from_millis(10);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
// Longer decoded audio cannot fit in the aligner's 2^22 10 ms frames.
const MAX_PCM_BYTES: u64 =
    crate::align::MAX_TIMELINE_FRAMES as u64 * (VIDEO_SAMPLE_RATE as u64 / 100) * 2;
pub const MAX_WAV_BYTES: u64 = MAX_PCM_BYTES + 4096;
const MAX_DIAGNOSTIC_BYTES: u64 = 1024 * 1024;

pub fn extract_pcm_via_ffmpeg(video_path: &Path, ffmpeg_bin: &str) -> Result<Vec<i16>, SyncError> {
    extract_pcm_via_ffmpeg_with_timeout(video_path, ffmpeg_bin, DEFAULT_FFMPEG_TIMEOUT)
}

pub fn extract_pcm_via_ffmpeg_with_timeout(
    video_path: &Path,
    ffmpeg_bin: &str,
    timeout: Duration,
) -> Result<Vec<i16>, SyncError> {
    check_cancelled()?;
    if timeout.is_zero() || timeout > MAX_FFMPEG_TIMEOUT {
        return Err(SyncError::Config(
            "FFmpeg timeout must be greater than 0 and at most 86400 seconds".into(),
        ));
    }
    let video = fs::canonicalize(video_path).map_err(|e| SyncError::io("Open video input", e))?;
    let mut command = Command::new(ffmpeg_bin);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(video)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-af",
            "aresample=async=1:first_pts=0",
            "-f",
            "s16le",
            "-",
        ]);
    let (status, stdout, stderr) = run_ffmpeg(command, timeout)?;
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let message = stderr.lines().next().unwrap_or("no error output");
        return Err(SyncError::Ffmpeg(format!("{status}: {message}")));
    }
    decode_pcm(&stdout)
}

fn run_ffmpeg(
    mut command: Command,
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), SyncError> {
    // Capture in anonymous files so inherited pipe handles cannot hang a reader during cleanup.
    let capture = |error| SyncError::io("Capture FFmpeg output", error);
    let (mut stdout, mut stderr) = (
        tempfile::tempfile().map_err(capture)?,
        tempfile::tempfile().map_err(capture)?,
    );
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().map_err(capture)?)
        .stderr(stderr.try_clone().map_err(capture)?);
    let mut command = CommandWrap::from(command);
    #[cfg(windows)]
    command
        .wrap(process_wrap::std::KillOnDrop)
        .wrap(process_wrap::std::JobObject);
    #[cfg(unix)]
    command.wrap(process_wrap::std::ProcessGroup::leader());
    let deadline = Instant::now() + timeout;
    check_cancelled()?;
    let mut child = command
        .spawn()
        .map_err(|e| SyncError::io("Start FFmpeg", e))?;
    drop(command);
    let outcome = await_ffmpeg(child.as_mut(), [&stdout, &stderr], deadline, timeout);
    let cleanup = terminate(child.as_mut());
    let status = match (outcome, cleanup) {
        (Ok(()), Ok(status)) => status,
        (Err(error), Ok(_)) => return Err(error),
        (Ok(()), Err(cleanup)) => {
            return Err(SyncError::io("Clean up FFmpeg process tree", cleanup));
        }
        (Err(error), Err(cleanup)) => {
            return Err(SyncError::Ffmpeg(format!(
                "{error}; FFmpeg process tree cleanup failed: {cleanup}"
            )));
        }
    };
    if stdout.metadata().map_err(capture)?.len() > MAX_PCM_BYTES
        || stderr.metadata().map_err(capture)?.len() > MAX_DIAGNOSTIC_BYTES
    {
        return Err(SyncError::Ffmpeg("output exceeded its byte limit".into()));
    }
    stdout.rewind().map_err(capture)?;
    stderr.rewind().map_err(capture)?;
    let (mut pcm, mut diagnostic) = (Vec::new(), Vec::new());
    stdout
        .take(MAX_PCM_BYTES + 1)
        .read_to_end(&mut pcm)
        .map_err(capture)?;
    stderr
        .take(8192)
        .read_to_end(&mut diagnostic)
        .map_err(capture)?;
    Ok((status, pcm, diagnostic))
}

fn await_ffmpeg(
    child: &mut dyn ChildWrapper,
    streams: [&File; 2],
    deadline: Instant,
    timeout: Duration,
) -> Result<(), SyncError> {
    loop {
        check_cancelled()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SyncError::Ffmpeg(format!(
                "timed out after {} seconds",
                timeout.as_secs_f64()
            )));
        }
        if leader_exited(child).map_err(|e| SyncError::io("Check FFmpeg status", e))? {
            return Ok(());
        }
        for (stream, limit) in streams
            .into_iter()
            .zip([MAX_PCM_BYTES, MAX_DIAGNOSTIC_BYTES])
        {
            if stream
                .metadata()
                .map_err(|e| SyncError::io("Check FFmpeg output size", e))?
                .len()
                > limit
            {
                return Err(SyncError::Ffmpeg("output exceeded its byte limit".into()));
            }
        }
        thread::sleep(POLL.min(remaining));
    }
}

#[cfg(unix)]
fn leader_exited(child: &mut dyn ChildWrapper) -> io::Result<bool> {
    use nix::{
        errno::Errno,
        sys::wait::{Id, WaitPidFlag, WaitStatus, waitid},
        unistd::Pid,
    };

    let pid = i32::try_from(child.id()).map_err(io::Error::other)?;
    // Keep the leader unreaped until the group is killed so its PID cannot be reused first.
    match waitid(
        Id::Pid(Pid::from_raw(pid)),
        WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
    ) {
        Ok(status) => Ok(status != WaitStatus::StillAlive),
        Err(Errno::EINTR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(windows)]
fn leader_exited(child: &mut dyn ChildWrapper) -> io::Result<bool> {
    Ok(child.inner_mut().try_wait()?.is_some())
}

fn terminate(child: &mut dyn ChildWrapper) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    match child.start_kill() {
        Ok(()) => (),
        Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32) => (),
        Err(error) => return Err(error),
    }
    #[cfg(windows)]
    if let Err(error) = child.start_kill() {
        return child.try_wait()?.ok_or(error);
    }
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => (),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "FFmpeg did not exit after termination",
            ));
        }
        thread::sleep(POLL.min(remaining));
    }
}

pub fn read_wav_pcm16(path: &Path) -> Result<(u32, Vec<i16>), SyncError> {
    let wav = crate::input::read_bytes(path, MAX_WAV_BYTES, "Read WAV input")?;
    if wav.get(..4) != Some(b"RIFF") || wav.get(8..12) != Some(b"WAVE") {
        return Err(SyncError::Audio("expected a RIFF/WAVE file".into()));
    }
    let declared = 8 + u64::from(u32_at(&wav, 4)?);
    let end = usize::try_from(declared)
        .ok()
        .filter(|&n| (12..=wav.len()).contains(&n))
        .ok_or_else(|| SyncError::Audio("invalid RIFF length".into()))?;
    let mut offset = 12;
    let mut rate = None;
    let mut data = None;
    while offset < end {
        let header = wav
            .get(offset..end)
            .and_then(|bytes| bytes.get(..8))
            .ok_or_else(|| SyncError::Audio("truncated WAV chunk header".into()))?;
        let len = u32_at(header, 4)? as usize;
        offset += 8;
        let chunk = wav
            .get(offset..end)
            .and_then(|bytes| bytes.get(..len))
            .ok_or_else(|| SyncError::Audio("truncated WAV chunk".into()))?;
        match &header[..4] {
            b"fmt " => {
                if chunk.len() < 16
                    || u16_at(chunk, 0)? != 1
                    || u16_at(chunk, 2)? != 1
                    || u16_at(chunk, 14)? != 16
                    || u16_at(chunk, 12)? != 2
                {
                    return Err(SyncError::Audio("WAV must be 16-bit mono PCM".into()));
                }
                rate = Some(wav_rate(u32_at(chunk, 4)?)?);
            }
            b"data" if data.is_none() => data = Some(chunk),
            _ => {}
        }
        offset += len;
        if !len.is_multiple_of(2) {
            if offset == end {
                return Err(SyncError::Audio("missing WAV padding byte".into()));
            }
            offset += 1;
        }
    }
    let rate = rate.ok_or_else(|| SyncError::Audio("missing WAV format chunk".into()))?;
    let data = data.ok_or_else(|| SyncError::Audio("missing WAV data chunk".into()))?;
    Ok((rate, decode_pcm(data)?))
}

pub fn write_wav_pcm16(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<(), SyncError> {
    let sample_rate = wav_rate(sample_rate)?;
    let len = u32::try_from(samples.len())
        .ok()
        .and_then(|len| len.checked_mul(2))
        .filter(|&len| len <= u32::MAX - 36)
        .ok_or_else(|| SyncError::Audio("samples exceed the RIFF size limit".into()))?;
    stage_file(path, |file| {
        let mut wav = BufWriter::new(file);
        wav.write_all(b"RIFF")?;
        wav.write_all(&(36 + len).to_le_bytes())?;
        wav.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0")?;
        wav.write_all(&sample_rate.to_le_bytes())?;
        wav.write_all(&(sample_rate * 2).to_le_bytes())?;
        wav.write_all(b"\x02\0\x10\0data")?;
        wav.write_all(&len.to_le_bytes())?;
        for sample in samples {
            wav.write_all(&sample.to_le_bytes())?;
        }
        wav.flush()
    })?
    .publish()
}

fn wav_rate(rate: u32) -> Result<u32, SyncError> {
    if (8000..=192_000).contains(&rate) {
        Ok(rate)
    } else {
        Err(SyncError::Audio(
            "WAV sample rate must be between 8000 and 192000 Hz".into(),
        ))
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, SyncError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| SyncError::Audio("truncated 16-bit WAV field".into()))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, SyncError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| SyncError::Audio("truncated 32-bit WAV field".into()))
}

fn decode_pcm(data: &[u8]) -> Result<Vec<i16>, SyncError> {
    if data.is_empty() || !data.len().is_multiple_of(2) {
        return Err(SyncError::Audio(
            "PCM data is empty or ends in a partial sample".into(),
        ));
    }
    Ok(data
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(i16::from_le_bytes)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs::OpenOptions};

    fn owned_fixture(directory: &Path, role: &str, finish: bool) -> Command {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "audio::tests::ffmpeg_timeout_kills_descendants_and_reaps_leader",
            ])
            .env("SUBSYNC_FFMPEG_FIXTURE_DIR", directory)
            .env("SUBSYNC_FFMPEG_FIXTURE_ROLE", role)
            .env(
                "SUBSYNC_FFMPEG_FIXTURE_FINISH",
                if finish { "1" } else { "0" },
            );
        command
    }

    fn marker(directory: &Path, role: &str) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(role))
            .unwrap()
    }

    #[test]
    #[expect(
        clippy::zombie_processes,
        reason = "The fixture intentionally exits before its descendant to verify owner cleanup"
    )]
    fn ffmpeg_timeout_kills_descendants_and_reaps_leader() {
        if let Some(directory) = env::var_os("SUBSYNC_FFMPEG_FIXTURE_DIR") {
            let directory = Path::new(&directory);
            let role = env::var("SUBSYNC_FFMPEG_FIXTURE_ROLE").unwrap();
            let finish = env::var("SUBSYNC_FFMPEG_FIXTURE_FINISH").unwrap() == "1";
            let lock = marker(directory, &role);
            lock.lock().unwrap();
            fs::write(directory.join(format!("{role}.ready")), b"ready").unwrap();
            if role == "leader" {
                let mut descendant = owned_fixture(directory, "descendant", finish)
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(3);
                while !directory.join("descendant.ready").is_file() {
                    if let Some(status) = descendant.try_wait().unwrap() {
                        panic!("descendant exited before it was ready: {status}");
                    }
                    assert!(Instant::now() < deadline, "descendant never started");
                    thread::sleep(POLL);
                }
                if finish {
                    return;
                }
            }
            // Bound the fixture independently so a broken supervisor cannot leave it running.
            thread::sleep(Duration::from_secs(9));
            return;
        }

        for finish in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let before = Instant::now();
            let outcome = run_ffmpeg(
                owned_fixture(directory.path(), "leader", finish),
                Duration::from_secs(3),
            );
            if finish {
                assert!(outcome.unwrap().0.success());
            } else {
                let error = outcome.unwrap_err().to_string();
                assert!(error.contains("timed out"), "{error}");
                assert!(!error.contains("cleanup failed"), "{error}");
            }
            assert!(before.elapsed() < Duration::from_secs(8));
            for role in ["leader", "descendant"] {
                assert!(directory.path().join(format!("{role}.ready")).is_file());
                let file = marker(directory.path(), role);
                let deadline = Instant::now() + Duration::from_secs(1);
                while file.try_lock().is_err() {
                    assert!(Instant::now() < deadline, "{role} is still running");
                    thread::sleep(POLL);
                }
            }
        }
    }
}
