# Import and playback fixtures

These fixtures contain generated data, including a test tone. They contain no personal music. Tests read the fixed bytes directly, so CI does not need an encoder or OpenSSL.

- `silence.mp3`: Three MPEG-1 Layer III frames with the header `ff fb 90 00`, each 417 bytes long, and zeroed side/main data. Used for audio recognition, byte preservation and MP3 playback tests.
- `tone.flac`: A generated one-second, 440 Hz tone at 44.1 kHz, mono, 16-bit, encoded with macOS `afconvert -f flac -d flac`.
- `cover.png`: A fixed 1×1 PNG.
- `fixture.ncm`: A fixed NCM container holding encrypted `silence.mp3` bytes, test metadata and `cover.png`. Its AES blocks were generated with OpenSSL. It has 16 reserved bytes after the artwork; unpacking must reproduce the MP3 bytes exactly.

Import and storage tests use these fixtures. Native playback tests also decode MP3/FLAC and consume the PCM through the Rodio mixer. Physical device tests and imports of personal MP3/FLAC/NCM files run separately and are ignored by default.
