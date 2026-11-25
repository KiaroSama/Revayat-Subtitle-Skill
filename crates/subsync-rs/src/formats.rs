use crate::SyncError;
use std::{ops::Range, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleFormat {
    Srt,
    Ass,
}

impl SubtitleFormat {
    pub(crate) fn precision(self) -> u64 {
        match self {
            Self::Srt => 1,
            Self::Ass => 10,
        }
    }

    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "ass" | "ssa" => Some(Self::Ass),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubtitleCue {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug)]
pub struct SubtitleDocument {
    pub format: SubtitleFormat,
    pub cues: Vec<SubtitleCue>,
    source: String,
    fields: Vec<[Range<usize>; 2]>,
}

impl SubtitleDocument {
    pub fn parse(text: &str, path: Option<&Path>) -> Result<Self, SyncError> {
        let text = text.trim_start_matches('\u{feff}');
        let format = path.and_then(SubtitleFormat::from_path).unwrap_or_else(|| {
            if source_lines(text).any(|(_, _, line)| {
                let line = line.trim();
                line.eq_ignore_ascii_case("[Script Info]") || line.eq_ignore_ascii_case("[Events]")
            }) {
                SubtitleFormat::Ass
            } else {
                SubtitleFormat::Srt
            }
        });
        let mut doc = Self {
            format,
            cues: Vec::new(),
            source: text.to_owned(),
            fields: Vec::new(),
        };
        match format {
            SubtitleFormat::Srt => doc.parse_srt(text)?,
            SubtitleFormat::Ass => doc.parse_ass(text)?,
        }
        if doc.cues.is_empty() {
            return Err(SyncError::Subtitle("no timed dialogue cues found".into()));
        }
        Ok(doc)
    }

    pub fn serialize(&self) -> Result<String, SyncError> {
        if self.cues.len() != self.fields.len() {
            return Err(SyncError::Subtitle("the number of cues changed".into()));
        }
        let unit = self.format.precision();
        let mut result = String::with_capacity(self.source.len());
        let mut cursor = 0;
        for (cue, [start, end]) in self.cues.iter().zip(&self.fields) {
            if cue.start_ms >= cue.end_ms {
                return Err(SyncError::Subtitle("cue end must follow its start".into()));
            }
            let start_ms = cue.start_ms / unit * unit;
            let end_ms = (cue.end_ms / unit * unit).max(start_ms.saturating_add(unit));
            if end_ms <= start_ms || !end_ms.is_multiple_of(unit) {
                return Err(SyncError::Subtitle("cue timestamp is out of range".into()));
            }
            let mut replacements = [(start, start_ms), (end, end_ms)];
            replacements.sort_unstable_by_key(|(field, _)| field.start);
            for (field, ms) in replacements {
                result.push_str(&self.source[cursor..field.start]);
                result.push_str(&format_time(ms, self.format));
                cursor = field.end;
            }
        }
        result.push_str(&self.source[cursor..]);
        Ok(result)
    }

    /// Compare all source bytes outside cue timestamp fields, including their order.
    pub fn has_same_untimed_content(&self, other: &Self) -> bool {
        if self.format != other.format || self.fields.len() != other.fields.len() {
            return false;
        }
        let (mut left, mut right) = (0, 0);
        for (fields, other_fields) in self.fields.iter().zip(&other.fields) {
            let mut fields = fields.each_ref();
            let mut other_fields = other_fields.each_ref();
            fields.sort_unstable_by_key(|field| field.start);
            other_fields.sort_unstable_by_key(|field| field.start);
            for (field, other_field) in fields.into_iter().zip(other_fields) {
                if self.source[left..field.start] != other.source[right..other_field.start] {
                    return false;
                }
                left = field.end;
                right = other_field.end;
            }
        }
        self.source[left..] == other.source[right..]
    }

    fn push_cue(
        &mut self,
        text: &str,
        fields: [Range<usize>; 2],
        line: usize,
    ) -> Result<(), SyncError> {
        let start_ms = parse_time(&text[fields[0].clone()], self.format)
            .ok_or_else(|| invalid_line(line, "invalid start timestamp"))?;
        let end_ms = parse_time(&text[fields[1].clone()], self.format)
            .ok_or_else(|| invalid_line(line, "invalid end timestamp"))?;
        if start_ms >= end_ms {
            return Err(invalid_line(line, "cue end must follow its start"));
        }
        self.cues.push(SubtitleCue { start_ms, end_ms });
        self.fields.push(fields);
        Ok(())
    }

    fn parse_srt(&mut self, text: &str) -> Result<(), SyncError> {
        let mut lines = source_lines(text).peekable();
        while let Some((line, base, value)) = lines.next() {
            if value.trim().is_empty() {
                continue;
            }
            let (line, base, value) = if value.trim().bytes().all(|byte| byte.is_ascii_digit()) {
                lines
                    .next()
                    .ok_or_else(|| invalid_line(line, "missing cue timing"))?
            } else {
                (line, base, value)
            };
            let (start, tail) = value
                .split_once("-->")
                .ok_or_else(|| invalid_line(line, "missing SRT timing arrow"))?;
            let end_offset = base + start.len() + 3;
            let tail_trimmed = tail.trim_start();
            let end = tail_trimmed.split_whitespace().next().unwrap_or("");
            self.push_cue(
                text,
                [
                    trimmed_range(base, start),
                    trimmed_range(end_offset + tail.len() - tail_trimmed.len(), end),
                ],
                line,
            )?;
            while lines
                .peek()
                .is_some_and(|(_, _, value)| !value.trim().is_empty())
            {
                let (line, _, value) = lines.next().expect("peeked SRT content line");
                if is_srt_timing(value) {
                    return Err(invalid_line(line, "missing SRT cue separator"));
                }
            }
        }
        Ok(())
    }

    fn parse_ass(&mut self, text: &str) -> Result<(), SyncError> {
        let mut events = false;
        let mut columns = (10, 1, 2, 9);
        for (line, base, value) in source_lines(text) {
            let trimmed = value.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                events = trimmed.eq_ignore_ascii_case("[Events]");
                columns = (10, 1, 2, 9);
                continue;
            }
            if !events {
                continue;
            }
            let Some((prefix, data)) = value.split_once(':') else {
                continue;
            };
            let prefix = prefix.trim();
            if prefix.eq_ignore_ascii_case("Format") {
                let names: Vec<_> = data.split(',').map(str::trim).collect();
                let unique = |name: &str| {
                    let mut matches = names
                        .iter()
                        .enumerate()
                        .filter_map(|(i, field)| field.eq_ignore_ascii_case(name).then_some(i));
                    let first = matches.next();
                    first.filter(|_| matches.next().is_none())
                };
                columns = match (unique("Start"), unique("End"), unique("Text")) {
                    (Some(start), Some(end), Some(text)) => (names.len(), start, end, text),
                    _ => {
                        return Err(invalid_line(
                            line,
                            "ASS Format must define Start, End and Text once",
                        ));
                    }
                };
            } else if prefix.eq_ignore_ascii_case("Dialogue") {
                let (count, start, end, text_column) = columns;
                let mut fields = Vec::with_capacity(count);
                let mut head = data;
                let mut offset = base + value.len() - data.len();
                for _ in 0..text_column {
                    let (field, rest) = head
                        .split_once(',')
                        .ok_or_else(|| invalid_line(line, "too few ASS dialogue fields"))?;
                    fields.push(trimmed_range(offset, field));
                    offset += field.len() + 1;
                    head = rest;
                }
                let mut tail_fields = Vec::with_capacity(count - text_column - 1);
                for _ in text_column + 1..count {
                    let (rest, field) = head
                        .rsplit_once(',')
                        .ok_or_else(|| invalid_line(line, "too few ASS dialogue fields"))?;
                    tail_fields.push(trimmed_range(offset + rest.len() + 1, field));
                    head = rest;
                }
                fields.push(offset..offset + head.len());
                fields.extend(tail_fields.into_iter().rev());
                self.push_cue(text, [fields[start].clone(), fields[end].clone()], line)?;
            }
        }
        Ok(())
    }
}

fn source_lines(text: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut offset = 0;
    let mut line = 0;
    std::iter::from_fn(move || {
        if offset == text.len() {
            return None;
        }
        let rest = &text[offset..];
        let length = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let base = offset;
        offset += length;
        // Count CRLF as one boundary and retain byte ranges for lossless replacement.
        if text[offset..].starts_with("\r\n") {
            offset += 2;
        } else if offset < text.len() {
            offset += 1;
        }
        line += 1;
        Some((line, base, &text[base..base + length]))
    })
}

fn is_srt_timing(value: &str) -> bool {
    let Some((start, end)) = value.split_once("-->") else {
        return false;
    };
    parse_time(start.trim(), SubtitleFormat::Srt).is_some()
        && end
            .split_whitespace()
            .next()
            .is_some_and(|end| parse_time(end, SubtitleFormat::Srt).is_some())
}

fn trimmed_range(base: usize, value: &str) -> Range<usize> {
    let trimmed = value.trim_start();
    let start = base + value.len() - trimmed.len();
    start..start + trimmed.trim_end().len()
}

fn invalid_line(line: usize, error: &str) -> SyncError {
    SyncError::Subtitle(format!("line {line}: {error}"))
}

fn parse_time(value: &str, format: SubtitleFormat) -> Option<u64> {
    let (whole, fraction) = value.split_once(match format {
        SubtitleFormat::Srt => ',',
        SubtitleFormat::Ass => '.',
    })?;
    let (hour, rest) = whole.split_once(':')?;
    let (minute, second) = rest.split_once(':')?;
    let parse = |s: &str| {
        s.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| s.parse::<u64>().ok())
            .flatten()
    };
    let digits = if format == SubtitleFormat::Srt { 3 } else { 2 };
    if minute.len() != 2 || second.len() != 2 || fraction.len() != digits {
        return None;
    }
    let (m, s, fraction) = (parse(minute)?, parse(second)?, parse(fraction)?);
    if m >= 60 || s >= 60 {
        return None;
    }
    parse(hour)?
        .checked_mul(3_600_000)?
        .checked_add(m * 60_000 + s * 1000 + fraction * format.precision())
}

fn format_time(ms: u64, format: SubtitleFormat) -> String {
    let (h, m, s) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60);
    match format {
        SubtitleFormat::Srt => format!("{h:02}:{m:02}:{s:02},{:03}", ms % 1000),
        SubtitleFormat::Ass => format!("{h}:{m:02}:{s:02}.{:02}", ms % 1000 / 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_mixed_line_endings_and_srt_position_fields() {
        let body = "7\r\n  01:00:00,024 --> 01:00:00,907 X1:33\n{\\an8}<b>x</b>\r\n<i>y</i>";
        let doc = SubtitleDocument::parse(body, None).unwrap();
        assert_eq!(doc.serialize().unwrap(), body);
    }

    #[test]
    fn preserves_cr_only_and_mixed_newlines_without_skipping_cues() {
        for newline in ["\r", "\r\n", "\n"] {
            let body = [
                "1",
                "00:00:01,000 --> 00:00:02,000",
                "First",
                "",
                "2",
                "00:00:03,000 --> 00:00:04,000",
                "Second",
                "",
            ]
            .join(newline);
            let doc = SubtitleDocument::parse(&body, None).unwrap();
            assert_eq!(doc.cues.len(), 2, "newline {newline:?}");
            assert_eq!(doc.serialize().unwrap(), body);
        }
        let body = "[Events]\rFormat: Start, End, Text\rDialogue: 0:00:01.00,0:00:02.00,First\nDialogue: 0:00:03.00,0:00:04.00,Second\r\n";
        let doc = SubtitleDocument::parse(body, None).unwrap();
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.serialize().unwrap(), body);
    }

    #[test]
    fn refuses_missing_srt_separator_instead_of_partially_retiming_a_document() {
        for body in [
            "1\n00:00:01,000 --> 00:00:02,000\nFirst\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n",
            "00:00:01,000 --> 00:00:02,000\nFirst\n00:00:03,000 --> 00:00:04,000\nSecond\n",
        ] {
            let error = SubtitleDocument::parse(body, None).unwrap_err();
            assert!(error.to_string().contains("separator"), "{error}");
        }
    }

    #[test]
    fn parses_ass_text_in_the_middle_with_commas_and_enforces_centiseconds() {
        let original = "[Events]\nFormat: End, Text, Start, Layer\nDialogue: 0:01:00.00,one,two,0:00:01.00,2\n";
        let mut doc = SubtitleDocument::parse(original, None).unwrap();
        doc.cues[0] = SubtitleCue {
            start_ms: 1001,
            end_ms: 1002,
        };
        assert!(
            doc.serialize()
                .unwrap()
                .contains("Dialogue: 0:00:01.01,one,two,0:00:01.00,2")
        );
    }
}
