use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleCue {
    pub id: String,
    pub start: i64,
    pub end: i64,
    pub text: String,
    pub fields: IndexMap<String, String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleDocument {
    pub kind: String,
    pub cues: Vec<Cue>,
    pub sections: Vec<(String, Vec<String>)>,
    pub event_fields: Vec<String>,
    pub style_fields: Vec<String>,
    pub styles: IndexMap<String, Vec<String>>,
    pub comments: usize,
    pub source_comments: IndexMap<String, Vec<String>>,
    pub chronological: bool,
    #[serde(skip)]
    pub bom: bool,
    #[serde(skip)]
    pub line_ending: LineEnding,
}

pub type Cue = SubtitleCue;
pub type Document = SubtitleDocument;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
    Mixed,
}

pub(super) fn line_ending(data: &str) -> LineEnding {
    let mut seen = None;
    let mut bytes = data.bytes().peekable();
    while let Some(byte) = bytes.next() {
        let ending = match byte {
            b'\r' if bytes.next_if_eq(&b'\n').is_some() => LineEnding::CrLf,
            b'\r' => LineEnding::Cr,
            b'\n' => LineEnding::Lf,
            _ => continue,
        };
        if seen.is_some_and(|previous| previous != ending) {
            return LineEnding::Mixed;
        }
        seen = Some(ending);
    }
    seen.unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleFormat {
    Ass,
    Srt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimestampCs(pub u64);

impl TimestampCs {
    pub fn from_millis(milliseconds: u64) -> Self {
        Self(milliseconds / 10)
    }
}

impl SubtitleCue {
    /// Whole centiseconds, floored from the original millisecond timing.
    pub fn start_cs(&self) -> u64 {
        TimestampCs::from_millis(u64::try_from(self.start).unwrap_or_default()).0
    }

    pub fn end_cs(&self) -> u64 {
        TimestampCs::from_millis(u64::try_from(self.end).unwrap_or_default()).0
    }
}

impl SubtitleDocument {
    pub fn format(&self) -> Option<SubtitleFormat> {
        match self.kind.as_str() {
            "ass" => Some(SubtitleFormat::Ass),
            "srt" => Some(SubtitleFormat::Srt),
            _ => None,
        }
    }
}
