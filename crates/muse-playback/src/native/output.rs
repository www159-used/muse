// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    num::{NonZeroU16, NonZeroU32},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source, mixer::Mixer};
use rtrb::Consumer;

use crate::{PlaybackError, PlaybackErrorKind as Kind, PlaybackState, PlaybackStatus};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
pub(super) struct DeviceFault {
    pub failed: AtomicBool,
    pub error: Mutex<Option<PlaybackError>>,
}

impl DeviceFault {
    pub fn set(&self, error: PlaybackError) {
        *lock(&self.error) = Some(error);
        self.failed.store(true, Ordering::Release);
    }
}

pub(super) struct Output {
    pub mixer: Mixer,
    pub fault: Arc<DeviceFault>,
    _device: Option<MixerDeviceSink>,
}

impl Output {
    pub fn open() -> Result<Self, PlaybackError> {
        let fault = Arc::new(DeviceFault::default());
        let callback_fault = Arc::clone(&fault);
        let mut device = DeviceSinkBuilder::from_default_device()
            .map_err(|error| PlaybackError::new(Kind::Output, error))?
            .with_error_callback(move |error| {
                callback_fault.set(PlaybackError::new(Kind::Output, error))
            })
            .open_sink_or_fallback()
            .map_err(|error| PlaybackError::new(Kind::Output, error))?;
        device.log_on_drop(false);
        Ok(Self {
            mixer: device.mixer().clone(),
            fault,
            _device: Some(device),
        })
    }

    #[cfg(test)]
    pub fn offline() -> (Self, rodio::mixer::MixerSource) {
        let (mixer, source) = rodio::mixer::mixer(
            NonZeroU16::new(2).unwrap(),
            NonZeroU32::new(44_100).unwrap(),
        );
        (
            Self {
                mixer,
                fault: Arc::new(DeviceFault::default()),
                _device: None,
            },
            source,
        )
    }
}

pub(super) struct Timeline {
    pub anchor: Duration,
    pub rate: u32,
    pub duration: Option<Duration>,
    pub failure: Option<PlaybackError>,
}

impl Timeline {
    pub fn status(&self, session: &Session) -> Result<PlaybackStatus, PlaybackError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let (state, frames) = session.snapshot();
        let position = self.anchor.saturating_add(Duration::from_secs_f64(
            frames as f64 / f64::from(self.rate),
        ));
        Ok(PlaybackStatus {
            state,
            position: self
                .duration
                .map_or(position, |duration| position.min(duration)),
            duration: self.duration,
        })
    }
}

pub(super) struct Session {
    pub active: AtomicBool,
    pub eof: AtomicBool,
    pub timeline: Mutex<Timeline>,
    // State and submitted frames change together, so pausing cannot race with
    // an in-flight frame and allow position to advance after pause completes.
    transport: AtomicU64,
}

const STATE_MASK: u64 = 7;
const FRAME_STEP: u64 = STATE_MASK + 1;

impl Session {
    pub fn new(state: PlaybackState, timeline: Timeline, eof: bool) -> Self {
        Self {
            active: AtomicBool::new(true),
            eof: AtomicBool::new(eof),
            timeline: Mutex::new(timeline),
            transport: AtomicU64::new(state as u64),
        }
    }

    pub fn snapshot(&self) -> (PlaybackState, u64) {
        let value = self.transport.load(Ordering::Acquire);
        let state = match value & STATE_MASK {
            value if value == PlaybackState::Ready as u64 => PlaybackState::Ready,
            value if value == PlaybackState::Playing as u64 => PlaybackState::Playing,
            value if value == PlaybackState::Paused as u64 => PlaybackState::Paused,
            value if value == PlaybackState::Finished as u64 => PlaybackState::Finished,
            _ => PlaybackState::Empty,
        };
        (state, value / FRAME_STEP)
    }

    pub fn state(&self) -> PlaybackState {
        self.snapshot().0
    }

    pub fn set_state(&self, state: PlaybackState) {
        let _ = self
            .transport
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                Some((value & !STATE_MASK) | state as u64)
            });
    }

    fn advance_frame(&self) -> bool {
        self.transport
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                if value & STATE_MASK == PlaybackState::Playing as u64 {
                    value.checked_add(FRAME_STEP)
                } else {
                    None
                }
            })
            .is_ok()
    }

    fn finish(&self) -> bool {
        self.transport
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                if value & STATE_MASK == PlaybackState::Playing as u64 {
                    Some((value & !STATE_MASK) | PlaybackState::Finished as u64)
                } else {
                    None
                }
            })
            .is_ok()
    }
}

pub(super) struct PcmSource {
    session: Arc<Session>,
    device_fault: Arc<DeviceFault>,
    pcm: Consumer<f32>,
    channels: NonZeroU16,
    rate: NonZeroU32,
    frame: Vec<f32>,
    index: usize,
    /// Application gain (`f32` bits). This is not the operating-system volume.
    gain: Arc<AtomicU32>,
}

impl PcmSource {
    pub fn new(
        session: Arc<Session>,
        fault: Arc<DeviceFault>,
        pcm: Consumer<f32>,
        channels: u16,
        rate: u32,
        gain: Arc<AtomicU32>,
    ) -> Self {
        Self {
            session,
            device_fault: fault,
            pcm,
            channels: NonZeroU16::new(channels).unwrap(),
            rate: NonZeroU32::new(rate).unwrap(),
            frame: vec![0.0; usize::from(channels)],
            index: usize::from(channels),
            gain,
        }
    }
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if !self.session.active.load(Ordering::Acquire)
            || self.device_fault.failed.load(Ordering::Acquire)
        {
            return None;
        }
        if self.index == self.frame.len() {
            self.frame.fill(0.0);
            match self.session.state() {
                PlaybackState::Playing => {
                    // Observe EOF before checking the queue: its release store
                    // follows the final publication of complete audio frames.
                    let eof = self.session.eof.load(Ordering::Acquire);
                    if let Ok(chunk) = self.pcm.read_chunk(self.frame.len()) {
                        if self.session.advance_frame() {
                            let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
                            for (sample, value) in self.frame.iter_mut().zip(chunk) {
                                *sample = value * gain;
                            }
                        }
                    } else if eof && self.session.finish() {
                        return None;
                    }
                }
                PlaybackState::Finished => return None,
                _ => {}
            }
            self.index = 0;
        }
        let sample = self.frame[self.index];
        self.index += 1;
        Some(sample)
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZeroU16 {
        self.channels
    }
    fn sample_rate(&self) -> NonZeroU32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
