// SPDX-License-Identifier: AGPL-3.0-only

// Same stand-in the service tests use: a long track that does not finish on its own.
use muse_playback::{AudioSource, Playback, PlaybackState, PlaybackStatus, SeekTarget};
use std::{
    io::Read,
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct ClockPlayer {
    status: Option<PlaybackStatus>,
    started: Option<Instant>,
}

impl Playback for ClockPlayer {
    type Error = std::io::Error;

    fn load(&mut self, mut source: Box<dyn AudioSource>) -> Result<(), Self::Error> {
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes)?;
        if bytes.is_empty() {
            return Err(std::io::Error::other("empty test audio"));
        }
        thread::sleep(Duration::from_millis(150));
        self.status = Some(PlaybackStatus {
            state: PlaybackState::Ready,
            position: Duration::ZERO,
            duration: Some(Duration::from_secs(3600)),
        });
        self.started = None;
        Ok(())
    }

    fn play(&mut self) -> Result<(), Self::Error> {
        let status = self
            .status
            .as_mut()
            .ok_or_else(|| std::io::Error::other("no track"))?;
        if status.state != PlaybackState::Playing {
            status.state = PlaybackState::Playing;
            self.started = Some(Instant::now());
        }
        Ok(())
    }

    fn pause(&mut self) -> Result<(), Self::Error> {
        let snapshot = self.status()?;
        if snapshot.state == PlaybackState::Playing {
            self.status = Some(PlaybackStatus {
                state: PlaybackState::Paused,
                ..snapshot
            });
            self.started = None;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        if let Some(status) = self.status.as_mut() {
            status.state = PlaybackState::Ready;
            status.position = Duration::ZERO;
        }
        self.started = None;
        Ok(())
    }

    fn seek(&mut self, target: SeekTarget) -> Result<Duration, Self::Error> {
        let mut status = self.status()?;
        status.position = match target {
            SeekTarget::Absolute(duration) => duration,
            SeekTarget::Forward(duration) => status.position.saturating_add(duration),
            SeekTarget::Backward(duration) => status.position.saturating_sub(duration),
        }
        .min(Duration::from_secs(3600));
        self.started = (status.state == PlaybackState::Playing).then(Instant::now);
        self.status = Some(status);
        Ok(status.position)
    }

    fn set_volume(&mut self, _volume: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn status(&self) -> Result<PlaybackStatus, Self::Error> {
        let mut status = self.status.unwrap_or(PlaybackStatus {
            state: PlaybackState::Empty,
            position: Duration::ZERO,
            duration: None,
        });
        if let Some(started) = self.started {
            status.position += started.elapsed();
        }
        Ok(status)
    }
}
