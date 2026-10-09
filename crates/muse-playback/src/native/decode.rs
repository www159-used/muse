// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{Arc, Mutex},
    time::Duration,
};

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_FLAC, CODEC_TYPE_MP3, Decoder, DecoderOptions},
    errors::Error as DecodeError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
    units::{Time, TimeBase},
};

use crate::{AudioSource, PlaybackError, PlaybackErrorKind as Kind};

struct ReaderState {
    input: Box<dyn AudioSource>,
    failure: Option<PlaybackError>,
}

#[derive(Clone)]
pub(super) struct OwnedReader {
    state: Arc<Mutex<ReaderState>>,
    length: u64,
}

impl OwnedReader {
    pub fn new(mut input: Box<dyn AudioSource>) -> Result<Self, PlaybackError> {
        let length = input
            .seek(SeekFrom::End(0))
            .map_err(|error| PlaybackError::new(Kind::Input, error))?;
        input
            .rewind()
            .map_err(|error| PlaybackError::new(Kind::Input, error))?;
        Ok(Self {
            state: Arc::new(Mutex::new(ReaderState {
                input,
                failure: None,
            })),
            length,
        })
    }

    fn rewind(&self) -> Result<(), PlaybackError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PlaybackError::message(Kind::Worker, "input lock poisoned"))?;
        state
            .input
            .rewind()
            .map_err(|error| PlaybackError::new(Kind::Input, error))?;
        state.failure = None;
        Ok(())
    }

    fn failure(&self) -> Option<PlaybackError> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.failure.clone())
    }
}

impl Read for OwnedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("input lock poisoned"))?;
        match state.input.read(buffer) {
            Ok(count) => Ok(count),
            Err(error) => {
                let returned = io::Error::new(error.kind(), error.to_string());
                state.failure = Some(PlaybackError::new(Kind::Input, error));
                Err(returned)
            }
        }
    }
}

impl Seek for OwnedReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("input lock poisoned"))?;
        match state.input.seek(position) {
            Ok(position) => Ok(position),
            Err(error) => {
                let returned = io::Error::new(error.kind(), error.to_string());
                state.failure = Some(PlaybackError::new(Kind::Input, error));
                Err(returned)
            }
        }
    }
}

impl MediaSource for OwnedReader {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}

pub(super) struct Decoded {
    pub input: OwnedReader,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    time_base: TimeBase,
    pub rate: u32,
    pub channels: u16,
    pub duration: Option<Duration>,
    pub pending: Vec<f32>,
    pub offset: usize,
    pub eof: bool,
    block_start: Duration,
    last_end: Duration,
    complete_stream: bool,
    is_flac: bool,
}

fn duration(time: Time) -> Duration {
    Duration::from_secs(time.seconds).saturating_add(Duration::from_secs_f64(time.frac))
}

impl Decoded {
    pub fn open(input: OwnedReader) -> Result<Self, PlaybackError> {
        input.rewind()?;
        let stream = MediaSourceStream::new(Box::new(input.clone()), Default::default());
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|error| {
                input
                    .failure()
                    .unwrap_or_else(|| PlaybackError::new(Kind::UnsupportedFormat, error))
            })?;
        let format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| PlaybackError::message(Kind::UnsupportedFormat, "no audio track"))?;
        let params = &track.codec_params;
        if !matches!(params.codec, CODEC_TYPE_MP3 | CODEC_TYPE_FLAC) {
            return Err(PlaybackError::message(
                Kind::UnsupportedFormat,
                "only MP3 and FLAC are supported",
            ));
        }
        let rate = params
            .sample_rate
            .filter(|rate| *rate > 0)
            .ok_or_else(|| PlaybackError::message(Kind::Decode, "missing sample rate"))?;
        let channels = params
            .channels
            .map(|channels| channels.count() as u16)
            .filter(|channels| *channels > 0)
            .ok_or_else(|| PlaybackError::message(Kind::Decode, "missing channels"))?;
        let time_base = params.time_base.unwrap_or_else(|| TimeBase::new(1, rate));
        let total = params
            .n_frames
            .map(|frames| duration(time_base.calc_time(frames)));
        let is_flac = params.codec == CODEC_TYPE_FLAC;
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions { verify: true })
            .map_err(|error| PlaybackError::new(Kind::Decode, error))?;
        let id = track.id;
        let mut decoded = Self {
            input,
            format,
            decoder,
            track: id,
            time_base,
            rate,
            channels,
            duration: total,
            pending: Vec::new(),
            offset: 0,
            eof: false,
            block_start: Duration::ZERO,
            last_end: Duration::ZERO,
            complete_stream: true,
            is_flac,
        };
        if !decoded.next_block()? {
            return Err(PlaybackError::message(
                Kind::Decode,
                "source has no decodable audio frames",
            ));
        }
        Ok(decoded)
    }

    pub fn next_block(&mut self) -> Result<bool, PlaybackError> {
        self.pending.clear();
        self.offset = 0;
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(error) => {
                    if let Some(failure) = self.input.failure() {
                        return Err(failure);
                    }
                    if matches!(&error, DecodeError::IoError(error) if error.kind() == io::ErrorKind::UnexpectedEof)
                    {
                        if self.complete_stream && self.decoder.finalize().verify_ok == Some(false)
                        {
                            return Err(PlaybackError::message(
                                Kind::Decode,
                                "audio integrity verification failed",
                            ));
                        }
                        let tolerance = Duration::from_secs_f64(1.0 / f64::from(self.rate));
                        if self.is_flac
                            && self.duration.is_some_and(|expected| {
                                self.last_end.saturating_add(tolerance) < expected
                            })
                        {
                            return Err(PlaybackError::message(
                                Kind::Decode,
                                "FLAC ended before its declared sample count",
                            ));
                        }
                        self.eof = true;
                        return Ok(false);
                    }
                    return Err(PlaybackError::new(Kind::Decode, error));
                }
            };
            if packet.track_id() != self.track {
                continue;
            }
            let audio = self
                .decoder
                .decode(&packet)
                .map_err(|error| PlaybackError::new(Kind::Decode, error))?;
            if audio.frames() == 0 {
                continue;
            }
            if audio.spec().rate != self.rate
                || audio.spec().channels.count() != usize::from(self.channels)
            {
                return Err(PlaybackError::message(
                    Kind::Decode,
                    "audio format changed mid-track",
                ));
            }
            let mut samples = SampleBuffer::<f32>::new(audio.capacity() as u64, *audio.spec());
            samples.copy_interleaved_ref(audio);
            self.pending.extend_from_slice(samples.samples());
            self.block_start = duration(self.time_base.calc_time(packet.ts()));
            self.last_end = self.block_start.saturating_add(Duration::from_secs_f64(
                (self.pending.len() / usize::from(self.channels)) as f64 / f64::from(self.rate),
            ));
            return Ok(true);
        }
    }

    pub fn seek(&mut self, target: Duration) -> Result<Duration, PlaybackError> {
        self.format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::new(
                        target.as_secs(),
                        f64::from(target.subsec_nanos()) / 1_000_000_000.0,
                    ),
                    track_id: Some(self.track),
                },
            )
            .map_err(|error| {
                self.input
                    .failure()
                    .unwrap_or_else(|| PlaybackError::new(Kind::UnsupportedSeek, error))
            })?;
        self.decoder.reset();
        // A whole-stream checksum cannot be validated after skipping samples.
        self.complete_stream = false;
        self.eof = false;
        while self.next_block()? {
            let frames = self.pending.len() / usize::from(self.channels);
            let end = self.block_start.saturating_add(Duration::from_secs_f64(
                frames as f64 / f64::from(self.rate),
            ));
            if end <= target {
                continue;
            }
            let discard = ((target.saturating_sub(self.block_start).as_secs_f64()
                * f64::from(self.rate))
            .floor() as usize)
                .min(frames);
            self.offset = discard * usize::from(self.channels);
            return Ok(self.block_start.saturating_add(Duration::from_secs_f64(
                discard as f64 / f64::from(self.rate),
            )));
        }
        Err(PlaybackError::message(
            Kind::UnsupportedSeek,
            "target beyond the decodable end of the source",
        ))
    }
}
