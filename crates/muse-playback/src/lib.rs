// SPDX-License-Identifier: AGPL-3.0-only

//! Playback interfaces for muse, with an optional built-in `native` backend.
//!
//! The caller supplies encoded audio and controls track selection. Future
//! implementations own decoding, device output and playback resources. The
//! optional `native` feature provides `NativePlayback`.
//!
//! The interface supports trait objects without naming an audio backend:
//!
//! ```
//! use muse_playback::{AudioSource, Playback};
//! use std::{error::Error, io::Cursor};
//!
//! fn resume<E: Error + Send + Sync + 'static>(
//!     player: &mut dyn Playback<Error = E>,
//! ) -> Result<(), E> {
//!     player.play()
//! }
//!
//! let source: Box<dyn AudioSource> = Box::new(Cursor::new(Vec::<u8>::new()));
//! // A Playback implementation takes ownership through load(source).
//! ```

use std::{
    error::Error,
    io::{Read, Seek},
    time::Duration,
};

#[cfg(feature = "native")]
mod error;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
pub use error::{PlaybackError, PlaybackErrorKind};
#[cfg(feature = "native")]
pub use native::NativePlayback;

/// An owned, seekable source of encoded audio, such as a file or memory cursor.
///
/// NCM must be unpacked before supplying it. Readers must outlive playback and
/// may be moved to an implementation's worker thread. Borrowed database BLOBs
/// require an owning reader or another owned source before they can be supplied.
/// Byte seeking on this reader does not itself guarantee audio-time seeking;
/// time navigation is expressed separately through [`Playback::seek`].
pub trait AudioSource: Read + Seek + Send + 'static {}

impl<T: Read + Seek + Send + 'static> AudioSource for T {}

/// A target on the loaded source's music timeline, expressed in time, not bytes.
///
/// Relative targets use the position when the implementation executes the seek.
/// Callers do not need to read a potentially stale status snapshot first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekTarget {
    /// Seeks to the given time measured from the beginning of the source.
    Absolute(Duration),
    /// Moves forward by the given duration from the execution-time position.
    Forward(Duration),
    /// Moves backward by the given duration from the execution-time position.
    Backward(Duration),
}

/// The state reported by a playback implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    /// No audio source is loaded.
    Empty,
    /// A source is loaded and positioned at the beginning.
    Ready,
    /// The loaded source is playing.
    Playing,
    /// Playback is paused at its current position.
    Paused,
    /// The loaded source reached its confirmed end through playback or seeking.
    Finished,
}

/// A snapshot of playback state and timing. Unknown total duration is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackStatus {
    pub state: PlaybackState,
    pub position: Duration,
    pub duration: Option<Duration>,
}

/// Maps the linear UI volume scale to a playback amplitude.
///
/// `volume` is the stored 0–100 command value. The amplitude multiplier is
/// `gain = (volume / 100)^2`, so equal UI steps stay closer to equal loudness
/// than linear amplitude. `0` mutes and `100` is unity gain. Values above 100
/// are clamped. This scales samples muse submits; it does not change the
/// operating-system mixer.
pub fn perceptual_gain(volume: u8) -> f32 {
    let unit = f32::from(volume.min(100)) / 100.0;
    unit * unit
}

/// Controls one loaded audio source, independently of storage and the UI.
///
/// All methods are required and have no default success behavior. The optional
/// native backend implements them. Implementations must release resources and stop
/// audio when they are dropped; the caller decides which track to play next.
pub trait Playback {
    /// The implementation's errors, exposed through the standard error trait.
    type Error: Error + Send + Sync + 'static;

    /// Takes ownership of a source, replacing the previous source on success.
    ///
    /// Success reports `Ready` at position zero without starting playback.
    /// Failure returns an error and must not report the new source as loaded.
    fn load(&mut self, source: Box<dyn AudioSource>) -> Result<(), Self::Error>;

    /// Starts a loaded source, resumes a paused source, or restarts a finished one.
    ///
    /// Calling this while playing is a no-op. Without a source, it returns an error.
    fn play(&mut self) -> Result<(), Self::Error>;

    /// Pauses playback, preserving its position. Other states remain unchanged.
    fn pause(&mut self) -> Result<(), Self::Error>;

    /// Stops playback, retains the source, and resets its position to zero.
    ///
    /// A loaded source becomes `Ready`; an empty player stays `Empty`.
    fn stop(&mut self) -> Result<(), Self::Error>;

    /// Seeks within the loaded source and returns the actual landing position.
    ///
    /// Success means seeking has completed, not merely that a request was queued.
    /// The actual position can differ from the target due to format or backend
    /// precision. It is a music-timeline position, not a promise about the exact
    /// instant audible at the device; use [`Self::status`] for later progress.
    /// The source is retained and the implementation never selects another track.
    ///
    /// Relative movement is calculated from the position when this operation is
    /// executed, using saturating arithmetic. Movement before the beginning clamps
    /// to zero; targets beyond a reliably known duration clamp to that duration.
    /// With an unknown duration, implementations must not invent a boundary from
    /// estimated metadata or scan the entire source just to honor the request.
    /// An unsupported or unreachable target returns an error rather than success.
    ///
    /// Successful seeking within the track preserves `Playing` or `Paused`.
    /// `Ready` stays `Ready` at zero and becomes `Paused` at a nonzero position.
    /// `Finished` becomes `Paused` when seeking back into the track, so a later
    /// [`Self::play`] resumes from that position. Seeking to the confirmed end
    /// reports `Finished` regardless of the previous loaded state.
    ///
    /// Without a source this returns an error. Rejection before seeking begins,
    /// including an unsupported operation, preserves a healthy source and its
    /// playback intent; a playing source may continue advancing naturally.
    /// Read, decode or output failures after seeking begins need not restore the
    /// exact previous position. Failures that prevent further playback must be
    /// reported by [`Self::status`] and must not be disguised as completion.
    ///
    /// Seeking can perform input and decoder work and may block. Interactive
    /// callers should run it on their playback control worker.
    ///
    /// # Examples
    ///
    /// All three targets work through the same backend-independent trait object:
    ///
    /// ```
    /// use muse_playback::{Playback, SeekTarget};
    /// use std::{error::Error, time::Duration};
    ///
    /// fn navigate<E: Error + Send + Sync + 'static>(
    ///     player: &mut dyn Playback<Error = E>,
    /// ) -> Result<Duration, E> {
    ///     // Jump to 90 seconds, then move relative to each execution-time position.
    ///     player.seek(SeekTarget::Absolute(Duration::from_secs(90)))?;
    ///     player.seek(SeekTarget::Forward(Duration::from_secs(10)))?;
    ///     // Return the actual landing position after moving back 10 seconds.
    ///     player.seek(SeekTarget::Backward(Duration::from_secs(10)))
    /// }
    /// ```
    fn seek(&mut self, target: SeekTarget) -> Result<Duration, Self::Error>;

    /// Returns current state and timing without starting or changing playback.
    ///
    /// Playback or seeking to the confirmed end reports `Finished`; playback
    /// errors are returned here if they occur after a control method has succeeded.
    fn status(&self) -> Result<PlaybackStatus, Self::Error>;

    /// Sets application-internal playback gain from the linear UI volume scale.
    ///
    /// `volume` uses the same 0–100 scale as the status `Volume:` field. The
    /// amplitude applied to output is [`perceptual_gain`], not a linear fraction
    /// of the operating-system volume. The setting survives later loads on this
    /// player. It does not change the system mixer.
    fn set_volume(&mut self, volume: u8) -> Result<(), Self::Error>;
}
