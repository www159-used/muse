# Synthetic import fixtures

No fixture contains personal music. Tests read fixed repository bytes and do not invoke encoders or the production decoder to construct expected results.

The immutable seeds `standard.ncm`, `silence.mp3`, `tone.flac` and `cover.png` originate from the generated library regression fixtures. The E2E package owns its corpus independently; the runner does not read sibling packages' test directories.

| Fixture | Purpose |
| --- | --- |
| `silence.mp3` | Three fixed MPEG-1 Layer III frames; golden MP3 bytes |
| `tone.flac` | Generated one-second 440 Hz tone at 44.1 kHz; golden FLAC bytes |
| `cover.png` | Fixed 1×1 PNG; standard embedded cover |
| `standard.ncm` | NCM containing `silence.mp3`, Fixture NCM/Fixture Artist/Fixture Album, NetEase ID 321 and `cover.png`; its cover frame contains 16 reserved bytes |
| `minimal.ncm` | No metadata, same MP3 and cover; filename fallback |
| `future-fields.ncm` | Korean title, Chinese album, two artists and unknown nested fields; explicit duration 23456 ms and ID 456 |
| `changed-types.ncm` | Unsupported title/artist/album/ID/duration shapes; valid audio with a filename fallback |
| `invalid-metadata.ncm` | Damaged metadata prefix; nonfatal metadata issue |
| `no-cover.ncm` | Metadata with ID 321, same MP3, no embedded cover; sidecar fallback |
| `embedded-lyrics.ncm` | Adds `[00:00]embedded lyrics`; embedded lyric priority |
| `flac.ncm` | Same title and metadata with the exact `tone.flac` payload |
| `other-artist-flac.ncm` | Same title, Other Artist and ID 654 with the exact `tone.flac` payload; a separate song |
| `tagged.mp3` | Golden MP3 plus a 128-byte ID3v1 tag with Changed title |
| `tagged-audio.ncm` | Same NCM title with `tagged.mp3`; complete-byte identity remains distinct |
| `same-song.mp3` | Golden MP3 with Fixture NCM/Fixture Artist/Fixture Album in ID3v1 tags |
| `other-artist.mp3` | Same title with Other Artist; separate tagged bytes |
| `case-song.mp3` | Lowercase title/artist and a different album; a metadata duplicate with different bytes |
| `blue.png` | A generated blue 1×1 PNG differing from the standard cover |

`regenerate.py` is an offline fixture-authoring tool using Python 3 and OpenSSL AES-128-ECB. It keeps the seed's encrypted audio key, derives the repeating mask from the fixed known MP3/ciphertext pair, and replaces declared metadata/audio/cover fields. It imports no muse code and is never run by CI. Expected track metadata is written independently in scenario files. Run it from any directory when intentionally editing the corpus:

```sh
python3 crates/muse-e2e/fixtures/regenerate.py
make e2e
```

Review both binary changes and explicit case expectations. A new downloaded client layout does not require regenerating fixtures; declare its paths in a new case. New formats require a new fixture and intended supported behavior or fallback case.
