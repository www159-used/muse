#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Regenerate synthetic NCM variants; never called by the test runner or CI.

Requires Python 3 and OpenSSL. The fixed standard.ncm and known MP3 bytes are
the seed: reuse its encrypted audio key, derive its mask from that known pair,
and replace only declared container fields. No muse code is imported.
"""

import base64
import json
from pathlib import Path
import struct
import subprocess
import zlib

ROOT = Path(__file__).resolve().parent
seed = (ROOT / "standard.ncm").read_bytes()
mp3 = (ROOT / "silence.mp3").read_bytes()
key_size = struct.unpack_from("<I", seed, 10)[0]
meta_offset = 14 + key_size
meta_size = struct.unpack_from("<I", seed, meta_offset)[0]
trailer = seed[meta_offset + 4 + meta_size:]
cover_frame_size = struct.unpack_from("<I", trailer, 5)[0]
audio_offset = 13 + cover_frame_size
encrypted_audio = trailer[audio_offset:]
assert len(encrypted_audio) == len(mp3) and len(mp3) >= 256
mask = bytes(left ^ right for left, right in zip(encrypted_audio[:256], mp3[:256]))
assert bytes(byte ^ mask[index % 256] for index, byte in enumerate(encrypted_audio)) == mp3


def encoded_metadata(value):
    if value is None:
        return b""
    clear = b"music:" + json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
    encrypted = subprocess.run(
        ["openssl", "enc", "-aes-128-ecb", "-K", b"#14ljk_!\\]&0U<'(".hex()],
        input=clear, capture_output=True, check=True,
    ).stdout
    return b"163 key(Don't modify):" + base64.b64encode(encrypted)


def write_variant(name, metadata, *, audio=None, cover=True, corrupt_metadata=False):
    encoded = b"invalid metadata prefix" if corrupt_metadata else encoded_metadata(metadata)
    encoded = bytes(byte ^ 0x63 for byte in encoded)
    frame = trailer[:audio_offset] if cover else trailer[:5] + struct.pack("<II", 0, 0)
    payload = encrypted_audio if audio is None else bytes(byte ^ mask[index % 256] for index, byte in enumerate(audio))
    result = seed[:meta_offset] + struct.pack("<I", len(encoded)) + encoded + frame + payload
    (ROOT / name).write_bytes(result)


standard_metadata = {
    "musicName": "Fixture NCM", "artist": [["Fixture Artist", 12]],
    "album": "Fixture Album", "musicId": 321, "duration": 12345,
}
write_variant("minimal.ncm", None)
write_variant("invalid-metadata.ncm", None, corrupt_metadata=True)
write_variant("no-cover.ncm", standard_metadata, cover=False)
write_variant("embedded-lyrics.ncm", {**standard_metadata, "lyric": "[00:00]embedded lyrics"})
write_variant("future-fields.ncm", {
    "musicName": "내일은 없어", "artist": [["Trouble Maker", 1], ["Guest", 2]],
    "album": "未来专辑", "musicId": 456, "duration": 23456,
    "futureContainerVersion": 99, "newClientData": {"unknown": [1, 2, 3]},
})
write_variant("changed-types.ncm", {
    "musicName": {"value": "unsupported shape"}, "artist": {"name": "unsupported shape"},
    "album": ["unsupported shape"], "musicId": "321", "duration": "12345",
})
write_variant("flac.ncm", standard_metadata, audio=(ROOT / "tone.flac").read_bytes())
id3v1 = b"TAG" + b"Changed title".ljust(30, b"\0") + b"\0" * 95
assert len(id3v1) == 128
tagged = mp3 + id3v1
(ROOT / "tagged.mp3").write_bytes(tagged)
write_variant("tagged-audio.ncm", standard_metadata, audio=tagged)


def png_chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


blue = b"\x89PNG\r\n\x1a\n"
blue += png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0))
blue += png_chunk(b"IDAT", zlib.compress(b"\x00\x00\x00\xff\xff"))
blue += png_chunk(b"IEND", b"")
(ROOT / "blue.png").write_bytes(blue)
print("Regenerated eight synthetic NCM variants, one tagged MP3 and one PNG fixture.")
