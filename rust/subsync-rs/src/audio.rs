use crate::SyncError;
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
    process::Command,
};

pub const VIDEO_SAMPLE_RATE: u32 = 16_000;

pub fn extract_pcm_via_ffmpeg(video_path: &Path, ffmpeg_bin: &str) -> Result<Vec<i16>, SyncError> {
    let video = fs::canonicalize(video_path).map_err(|e| SyncError::io("Open video input", e))?;
    let output = Command::new(ffmpeg_bin)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(video)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
        .output()
        .map_err(|e| SyncError::io("Start FFmpeg", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.lines().next().unwrap_or("no error output");
        return Err(SyncError::Ffmpeg(format!("{}: {message}", output.status)));
    }
    decode_pcm(&output.stdout)
}

pub fn read_wav_pcm16(path: &Path) -> Result<(u32, Vec<i16>), SyncError> {
    let wav = fs::read(path).map_err(|e| SyncError::io("Read WAV input", e))?;
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
    let write = || -> std::io::Result<()> {
        let mut wav = BufWriter::new(fs::File::create_new(path)?);
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
    };
    write().map_err(|e| SyncError::io("Write new WAV file", e))
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
