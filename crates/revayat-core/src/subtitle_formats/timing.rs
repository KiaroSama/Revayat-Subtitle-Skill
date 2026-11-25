use super::invalid;

const MAX_TIME_MS: i64 = crate::validation::MAX_TIME_MS;

pub fn timestamp(input: &str, kind: &str) -> Result<i64, String> {
    let (sep, digits, factor) = match kind {
        "ass" => ('.', 2, 10),
        "srt" => (',', 3, 1),
        _ => return Err(invalid("Invalid subtitle timestamp or format")),
    };
    let error = || invalid("Invalid subtitle timestamp");
    let (whole, frac) = input
        .trim_matches([' ', '\t'])
        .split_once(sep)
        .ok_or_else(error)?;
    let mut p = whole.split(':');
    let h = p
        .next()
        .filter(|h| (1..=12).contains(&h.len()))
        .ok_or_else(error)?;
    let m = p.next().filter(|p| p.len() == 2).ok_or_else(error)?;
    let s = p.next().filter(|s| s.len() == 2).ok_or_else(error)?;
    if p.next().is_some()
        || frac.len() != digits
        || [h, m, s, frac]
            .iter()
            .any(|s| !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(error());
    }
    let mut parsed = [0_i64; 4];
    for (out, part) in parsed.iter_mut().zip([h, m, s, frac]) {
        *out = part.parse::<i64>().map_err(|_| error())?;
    }
    let [h, m, s, f] = parsed;
    if m > 59 || s > 59 {
        return Err(invalid("Subtitle minutes and seconds must be below 60"));
    }
    let ms = ((h * 60 + m) * 60 + s) * 1000 + f * factor;
    if ms > MAX_TIME_MS {
        return Err(invalid(
            "Subtitle timestamp exceeds the supported millisecond limit",
        ));
    }
    Ok(ms)
}
pub fn timecode(ms: i64, kind: &str) -> Result<String, String> {
    if !(0..=MAX_TIME_MS).contains(&ms) || !matches!(kind, "ass" | "srt") {
        return Err(invalid(
            "Timestamp serialization requires nonnegative integer milliseconds and ASS/SRT",
        ));
    }
    let (h, m, s, f) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    Ok(if kind == "ass" {
        format!("{h}:{m:02}:{s:02}.{:02}", f / 10)
    } else {
        format!("{h:02}:{m:02}:{s:02},{f:03}")
    })
}
pub fn effective_times(start: i64, end: i64, kind: &str) -> Result<(i64, i64), String> {
    if !matches!(kind, "ass" | "srt") {
        return Err(invalid("Cue timing requires ASS or SRT format"));
    }
    if start < 0 || end <= start || end > MAX_TIME_MS {
        return Err(invalid(
            "Cue timing requires nonnegative integers and end after start",
        ));
    }
    let (start, end) = if kind == "ass" {
        (start / 10 * 10, end / 10 * 10)
    } else {
        (start, end)
    };
    if end <= start {
        return Err(invalid(
            "Centisecond quantization collapses this cue; review its timing explicitly",
        ));
    }
    Ok((start, end))
}
