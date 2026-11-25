//! Checks the renderer's constrained PNG profile without reconstructing pixels.

#![forbid(unsafe_code)]

use flate2::{Decompress, FlushDecompress, Status};
use serde_json::json;
use std::time::{Duration, Instant};

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
pub const MAX_PNG_BYTES: usize = 64 * 1024 * 1024;
const MAX_PIXELS: u32 = 16 * 1024 * 1024;

fn invalid(reason: &str) -> String {
    format!("ValueError: PNG evidence: {reason}")
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

pub fn decode_png(data: Vec<u8>) -> Result<serde_json::Value, String> {
    let (width, height) = decode_dimensions(&data)?;
    Ok(json!({"$tuple": [width, height]}))
}

pub fn decode_dimensions(data: &[u8]) -> Result<(u32, u32), String> {
    if data.len() > MAX_PNG_BYTES || !data.starts_with(SIGNATURE) {
        return Err(invalid("invalid signature or file size"));
    }
    let mut rest = &data[SIGNATURE.len()..];
    let mut header = None;
    let mut count = 0;
    let mut compressed = Vec::new();
    let (mut saw_idat, mut image_ended, mut palette_seen) = (false, false, false);
    let (width, height, channels) = loop {
        if rest.is_empty() {
            return Err(invalid("missing header, image data or end marker"));
        }
        if rest.len() < 12 {
            return Err(invalid("truncated chunk"));
        }
        let size = be_u32(rest) as usize;
        let kind = &rest[4..8];
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2].is_ascii_lowercase() {
            return Err(invalid("invalid chunk type"));
        }
        if size > rest.len() - 12 {
            return Err(invalid("truncated chunk body"));
        }
        let body_end = 8 + size;
        let body = &rest[8..body_end];
        if crc32fast::hash(&rest[4..body_end]) != be_u32(&rest[body_end..]) {
            return Err(invalid("chunk CRC mismatch"));
        }
        rest = &rest[body_end + 4..];
        count += 1;
        if count > 4096 || (header.is_none() && kind != b"IHDR") {
            return Err(invalid("invalid chunk sequence or excessive chunks"));
        }
        match kind {
            b"IHDR" => {
                if header.is_some() || size != 13 {
                    return Err(invalid("invalid or repeated header"));
                }
                let (width, height) = (be_u32(body), be_u32(&body[4..]));
                if !(1..=8192).contains(&width)
                    || !(1..=8192).contains(&height)
                    || width * height > MAX_PIXELS
                {
                    return Err(invalid("dimensions exceed the supported render budget"));
                }
                if !matches!(body[8..], [8, 2 | 6, 0, 0, 0]) {
                    return Err(invalid("rerender as noninterlaced 8-bit RGB/RGBA"));
                }
                header = Some((width, height, if body[9] == 2 { 3 } else { 4 }));
            }
            b"IDAT" => {
                if image_ended {
                    return Err(invalid("noncontiguous image data"));
                }
                saw_idat = true;
                compressed.extend_from_slice(body);
            }
            b"IEND" => {
                if size != 0 || !saw_idat || !rest.is_empty() {
                    return Err(invalid("invalid end marker or trailing bytes"));
                }
                break header.ok_or_else(|| invalid("missing header, image data or end marker"))?;
            }
            _ => {
                image_ended |= saw_idat;
                match kind {
                    b"acTL" | b"fcTL" | b"fdAT" => {
                        return Err(invalid("animated images are not single-frame evidence"));
                    }
                    b"PLTE"
                        if palette_seen
                            || saw_idat
                            || size == 0
                            || !size.is_multiple_of(3)
                            || size > 768 =>
                    {
                        return Err(invalid("invalid or repeated palette"));
                    }
                    b"PLTE" => palette_seen = true,
                    _ if kind[0].is_ascii_uppercase() => {
                        return Err(invalid("unsupported critical chunk"));
                    }
                    _ => {}
                }
            }
        }
    };

    let stride = (width * channels + 1) as usize;
    let expected = stride * height as usize;
    let mut pixels = vec![0; expected + 1];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(&compressed, &mut pixels, FlushDecompress::Sync)
        .map_err(|_| invalid("invalid compressed image data"))?;
    if status != Status::StreamEnd
        || decoder.total_in() != compressed.len() as u64
        || decoder.total_out() != expected as u64
    {
        return Err(invalid("truncated, oversized or concatenated image stream"));
    }
    let start = Instant::now();
    for row in pixels[..expected].chunks_exact(stride) {
        if start.elapsed() > Duration::from_secs(20) {
            return Err(invalid("decode exceeded its time budget"));
        }
        if row[0] > 4 {
            return Err(invalid("unknown scanline filter"));
        }
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::decode_png;
    use flate2::{Compression, write::ZlibEncoder};
    use serde_json::json;
    use std::io::Write;

    fn chunk(image: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
        image.extend((body.len() as u32).to_be_bytes());
        let start = image.len();
        image.extend(kind);
        image.extend(body);
        image.extend(crc32fast::hash(&image[start..]).to_be_bytes());
    }

    fn compress(pixels: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(pixels)?;
        encoder.finish()
    }

    fn png(color: u8, stream: &[u8]) -> Vec<u8> {
        let mut image = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(
            &mut image,
            b"IHDR",
            &[0, 0, 0, 2, 0, 0, 0, 1, 8, color, 0, 0, 0],
        );
        let (first, rest) = stream.split_at(stream.len() / 2);
        chunk(&mut image, b"IDAT", first);
        chunk(&mut image, b"IDAT", rest);
        chunk(&mut image, b"IEND", &[]);
        image
    }

    #[test]
    fn accepts_split_rgb_and_rgba_streams_with_each_legal_filter() -> std::io::Result<()> {
        for (color, stride) in [(2, 7), (6, 9)] {
            for filter in 0..=4 {
                let mut pixels = vec![0; stride];
                pixels[0] = filter;
                assert_eq!(
                    decode_png(png(color, &compress(&pixels)?)),
                    Ok(json!({"$tuple": [2, 1]}))
                );
            }
        }
        Ok(())
    }

    #[test]
    fn refuses_trailing_zlib_bytes_and_unknown_filters() -> std::io::Result<()> {
        let mut stream = compress(&[0; 7])?;
        stream.push(0);
        assert_eq!(
            decode_png(png(2, &stream)),
            Err(
                "ValueError: PNG evidence: truncated, oversized or concatenated image stream"
                    .into()
            )
        );
        let stream = compress(&[5, 0, 0, 0, 0, 0, 0])?;
        assert_eq!(
            decode_png(png(2, &stream)),
            Err("ValueError: PNG evidence: unknown scanline filter".into())
        );
        Ok(())
    }
}
