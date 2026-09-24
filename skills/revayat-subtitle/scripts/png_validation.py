"""Bounded decoding of the renderer's noninterlaced 8-bit RGB/RGBA PNG profile."""

from __future__ import annotations

import struct
import time
import zlib

MAX_PNG_BYTES = 64 * 1024 * 1024
MAX_PIXELS = 16 * 1024 * 1024


def decode_png(data: bytes) -> tuple[int, int]:
    def invalid(reason):
        raise ValueError("PNG evidence: " + reason)
    if len(data) > MAX_PNG_BYTES or data[:8] != b"\x89PNG\r\n\x1a\n":
        invalid("invalid signature or file size")
    cursor, count = 8, 0
    header = None
    compressed = []
    ended = False
    image_ended = False
    palette_seen = False
    while cursor < len(data):
        if cursor + 12 > len(data):
            invalid("truncated chunk")
        size = struct.unpack_from(">I", data, cursor)[0]
        kind = data[cursor + 4:cursor + 8]
        if not all(65 <= value <= 90 or 97 <= value <= 122 for value in kind) or kind[2] & 32:
            invalid("invalid chunk type")
        end = cursor + 12 + size
        if end > len(data):
            invalid("truncated chunk body")
        body = data[cursor + 8:cursor + 8 + size]
        crc = struct.unpack_from(">I", data, cursor + 8 + size)[0]
        if zlib.crc32(body, zlib.crc32(kind)) != crc:
            invalid("chunk CRC mismatch")
        count += 1
        if count > 4096 or (header is None and kind != b"IHDR"):
            invalid("invalid chunk sequence or excessive chunks")
        if kind == b"IHDR":
            if header is not None or size != 13:
                invalid("invalid or repeated header")
            header = struct.unpack(">IIBBBBB", body)
            width, height, depth, color, compression, filtering, interlace = header
            if not 0 < width <= 8192 or not 0 < height <= 8192 or width * height > MAX_PIXELS:
                invalid("dimensions exceed the supported render budget")
            if depth != 8 or color not in (2, 6) or compression or filtering or interlace:
                invalid("rerender as noninterlaced 8-bit RGB/RGBA")
        elif kind == b"IDAT":
            if image_ended:
                invalid("noncontiguous image data")
            compressed.append(body)
        elif kind == b"IEND":
            if size or not compressed or end != len(data):
                invalid("invalid end marker or trailing bytes")
            ended = True
            cursor = end
            break
        else:
            if compressed:
                image_ended = True
            if kind in (b"acTL", b"fcTL", b"fdAT"):
                invalid("animated images are not single-frame evidence")
            if kind == b"PLTE":
                if palette_seen or compressed or not size or size % 3 or size > 768:
                    invalid("invalid or repeated palette")
                palette_seen = True
            elif not kind or not (kind[0] & 32):
                invalid("unsupported critical chunk")
        cursor = end
    if not ended or header is None:
        invalid("missing header, image data or end marker")
    width, height, _, color, *_ = header
    channels = 3 if color == 2 else 4
    stride = width * channels
    expected = (stride + 1) * height
    decoder = zlib.decompressobj()
    try:
        pixels = decoder.decompress(b"".join(compressed), expected + 1)
    except zlib.error:
        invalid("invalid compressed image data")
    if len(pixels) != expected or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:
        invalid("truncated, oversized or concatenated image stream")
    previous = bytearray(stride)
    deadline = time.monotonic() + 20
    for row_index in range(height):
        if time.monotonic() > deadline:
            invalid("decode exceeded its time budget")
        offset = row_index * (stride + 1)
        filter_ = pixels[offset]
        row = bytearray(pixels[offset + 1:offset + 1 + stride])
        if filter_ == 2:
            row = bytearray((value + above) & 255 for value, above in zip(row, previous))
        elif filter_ in (1, 3, 4):
            for index in range(stride):
                left = row[index - channels] if index >= channels else 0
                above = previous[index]
                upper_left = previous[index - channels] if index >= channels else 0
                if filter_ == 1:
                    predictor = left
                elif filter_ == 3:
                    predictor = (left + above) // 2
                else:
                    base = left + above - upper_left
                    a, b, c = abs(base - left), abs(base - above), abs(base - upper_left)
                    predictor = left if a <= b and a <= c else above if b <= c else upper_left
                row[index] = (row[index] + predictor) & 255
        elif filter_ != 0:
            invalid("unknown scanline filter")
        previous = row
    return width, height
