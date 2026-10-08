"""Independent constrained PNG profile fixtures, including every legal filter."""
import struct
import unittest
import zlib
from unittest.mock import patch

import png_validation


def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))


def png(width=3, height=2, color=2, filter_=0, *, pixels=None, compressed=None, depth=8, interlace=0):
    channels = 3 if color == 2 else 4
    if pixels is None:
        pixels = (bytes([filter_]) + bytes(range(width * channels))) * height
    stream = zlib.compress(pixels) if compressed is None else compressed
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, color, 0, 0, interlace))
            + chunk(b"IDAT", stream[:len(stream) // 2]) + chunk(b"IDAT", stream[len(stream) // 2:]) + chunk(b"IEND", b""))


class PngProfileTests(unittest.TestCase):
    def test_rgb_rgba_all_filters_first_and_following_rows(self):
        for color in (2, 6):
            for filter_ in range(5):
                with self.subTest(color=color, filter=filter_):
                    self.assertEqual(png_validation.decode_png(png(color=color, filter_=filter_)), (3, 2))

    def test_invalid_filters_and_stream_lengths_refuse(self):
        for filter_ in (5, 255):
            with self.subTest(filter=filter_), self.assertRaisesRegex(ValueError, "filter"):
                png_validation.decode_png(png(filter_=filter_))
        for pixels in (b"", b"\0" * 19, b"\0" * 21):
            with self.subTest(size=len(pixels)), self.assertRaisesRegex(ValueError, "stream"):
                png_validation.decode_png(png(pixels=pixels))
        for stream in (zlib.compress(b"\0" * 20)[:-1], zlib.compress(b"\0" * 20) + b"tail",
                       zlib.compress(b"\0" * 20) + zlib.compress(b"")):
            with self.subTest(stream_size=len(stream)), self.assertRaisesRegex(ValueError, "stream"):
                png_validation.decode_png(png(compressed=stream))

    def test_crc_profile_order_and_caps_remain_load_bearing(self):
        data = bytearray(png())
        data[-1] ^= 1
        cases = ((bytes(data), "CRC"), (png(depth=16), "RGB/RGBA"), (png(interlace=1), "RGB/RGBA"),
                 (png() + b"tail", "trailing"), (png(width=8193, pixels=b""), "dimensions"))
        for data, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                png_validation.decode_png(data)
        with patch.object(png_validation, "MAX_PNG_BYTES", 8), self.assertRaisesRegex(ValueError, "size"):
            png_validation.decode_png(png())
        with patch.object(png_validation, "MAX_PIXELS", 1), self.assertRaisesRegex(ValueError, "dimensions"):
            png_validation.decode_png(png())


if __name__ == "__main__":
    unittest.main()
