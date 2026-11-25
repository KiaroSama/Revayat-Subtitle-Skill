//! One codec registry for import preflight and strict byte decoding.

use encoding_rs::Encoding;

pub(crate) enum Codec {
    Utf8(bool),
    Ascii,
    Latin1,
    Utf16(Option<bool>),
    Utf32(Option<bool>),
    Legacy(&'static Encoding),
}

pub(crate) fn resolve(label: &str) -> Option<Codec> {
    if label.trim() != label {
        return None;
    }
    let key = label.to_ascii_lowercase().replace('_', "-");
    Some(match key.as_str() {
        "utf-8" | "utf8" => Codec::Utf8(false),
        "utf-8-sig" => Codec::Utf8(true),
        "ascii" | "us-ascii" | "ansi-x3.4-1968" | "ansi-x3.4-1986" | "iso-ir-6" | "iso646-us"
        | "iso646.irv:1991" | "ibm367" | "cp367" | "csascii" => Codec::Ascii,
        "latin-1" | "latin1" | "iso-8859-1" | "iso8859-1" | "iso-8859-1:1987" | "iso-ir-100"
        | "l1" | "ibm819" | "cp819" | "csisolatin1" => Codec::Latin1,
        "utf-16" | "utf16" => Codec::Utf16(None),
        "utf-16-le" | "utf16-le" | "utf-16le" | "utf16le" => Codec::Utf16(Some(true)),
        "utf-16-be" | "utf16-be" | "utf-16be" | "utf16be" => Codec::Utf16(Some(false)),
        "utf-32" | "utf32" => Codec::Utf32(None),
        "utf-32-le" | "utf32-le" | "utf-32le" | "utf32le" => Codec::Utf32(Some(true)),
        "utf-32-be" | "utf32-be" | "utf-32be" | "utf32be" => Codec::Utf32(Some(false)),
        "replacement" | "x-user-defined" | "unicodefffe" | "unicodefeff" => return None,
        _ => {
            let codec = Encoding::for_label(key.as_bytes()).or_else(|| {
                key.strip_prefix("cp")
                    .and_then(|suffix| Encoding::for_label(format!("windows-{suffix}").as_bytes()))
            })?;
            if codec == encoding_rs::REPLACEMENT || codec == encoding_rs::X_USER_DEFINED {
                return None;
            }
            Codec::Legacy(codec)
        }
    })
}

pub(crate) fn decode(raw: &[u8], label: &str) -> Result<String, String> {
    let invalid = || format!("UnicodeDecodeError: invalid {label} subtitle data");
    let codec = resolve(label).ok_or_else(|| {
        "ValueError: encoding: expected a supported text encoding, not a binary transform"
            .to_owned()
    })?;
    match codec {
        Codec::Utf8(signature) => std::str::from_utf8(if signature {
            raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw)
        } else {
            raw
        })
        .map(str::to_owned)
        .map_err(|_| invalid()),
        Codec::Ascii if !raw.is_ascii() => Err(invalid()),
        Codec::Ascii | Codec::Latin1 => Ok(raw.iter().map(|&b| char::from(b)).collect()),
        Codec::Utf16(endian) => {
            let (little, raw) = match (endian, raw) {
                (Some(little), raw) => (little, raw),
                (None, [0xff, 0xfe, tail @ ..]) => (true, tail),
                (None, [0xfe, 0xff, tail @ ..]) => (false, tail),
                _ => return Err(invalid()),
            };
            if raw.len() % 2 != 0 {
                return Err(invalid());
            }
            let units = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&unit| {
                    if little {
                        u16::from_le_bytes(unit)
                    } else {
                        u16::from_be_bytes(unit)
                    }
                })
                .collect::<Vec<_>>();
            String::from_utf16(&units).map_err(|_| invalid())
        }
        Codec::Utf32(endian) => {
            let (little, raw) = match (endian, raw) {
                (Some(little), raw) => (little, raw),
                (None, [0xff, 0xfe, 0, 0, tail @ ..]) => (true, tail),
                (None, [0, 0, 0xfe, 0xff, tail @ ..]) => (false, tail),
                _ => return Err(invalid()),
            };
            if raw.len() % 4 != 0 {
                return Err(invalid());
            }
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|&unit| {
                    let value = if little {
                        u32::from_le_bytes(unit)
                    } else {
                        u32::from_be_bytes(unit)
                    };
                    char::from_u32(value).ok_or_else(invalid)
                })
                .collect()
        }
        Codec::Legacy(codec) => codec
            .decode_without_bom_handling_and_without_replacement(raw)
            .map(|text| text.into_owned())
            .ok_or_else(invalid),
    }
}
