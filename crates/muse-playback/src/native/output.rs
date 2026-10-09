// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    collections::VecDeque,
    num::{NonZeroU16, NonZeroU32},
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source, mixer::Mixer};

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

pub(super) struct Buffer {
    pub state: PlaybackState,
    pub anchor: Duration,
    pub frames: u64,
    pub rate: u32,
    pub duration: Option<Duration>,
    pub pcm: VecDeque<f32>,
    pub eof: bool,
    pub failure: Option<PlaybackError>,
}

impl Buffer {
    pub fn status(&self) -> Result<PlaybackStatus, PlaybackError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let position = self.anchor.saturating_add(Duration::from_secs_f64(
            self.frames as f64 / f64::from(self.rate),
        ));
        Ok(PlaybackStatus {
            state: self.state,
            position: self
                .duration
                .map_or(position, |duration| position.min(duration)),
            duration: self.duration,
        })
    }
}

pub(super) struct Session {
    pub active: AtomicBool,
    pub buffer: Mutex<Buffer>,
}

pub(super) struct PcmSource {
    session: Arc<Session>,
    device_fault: Arc<DeviceFault>,
    channels: NonZeroU16,
    rate: NonZeroU32,
    frame: Vec<f32>,
    index: usize,
}

impl PcmSource {
    pub fn new(session: Arc<Session>, fault: Arc<DeviceFault>, channels: u16, rate: u32) -> Self {
        Self {
            session,
            device_fault: fault,
            channels: NonZeroU16::new(channels).unwrap(),
            rate: NonZeroU32::new(rate).unwrap(),
            frame: vec![0.0; usize::from(channels)],
            index: usize::from(channels),
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
            match self.session.buffer.try_lock() {
                Ok(mut buffer) => {
                    if buffer.failure.is_some() {
                        return None;
                    }
                    if buffer.state == PlaybackState::Playing {
                        if buffer.pcm.len() >= self.frame.len() {
                            for sample in &mut self.frame {
                                *sample = buffer.pcm.pop_front().unwrap();
                            }
                            buffer.frames = buffer.frames.saturating_add(1);
                        } else if buffer.eof {
                            buffer.state = PlaybackState::Finished;
                            return None;
                        }
                    } else if buffer.state == PlaybackState::Finished {
                        return None;
                    }
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Poisoned(_)) => return None,
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
