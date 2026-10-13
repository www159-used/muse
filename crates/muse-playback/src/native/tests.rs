// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::perceptual_gain;
use std::{
    cell::Cell,
    io::{self, Cursor, Read, Seek, SeekFrom},
    sync::atomic::AtomicU32,
    time::Instant,
};

fn unity_gain() -> Arc<AtomicU32> {
    Arc::new(AtomicU32::new(perceptual_gain(100).to_bits()))
}

const MP3: &[u8] = include_bytes!("../../../muse-library/tests/fixtures/silence.mp3");
const FLAC: &[u8] = include_bytes!("../../../muse-library/tests/fixtures/tone.flac");

#[test]
fn half_volume_uses_square_law_gain_on_submitted_samples() {
    assert_eq!(perceptual_gain(0), 0.0);
    assert_eq!(perceptual_gain(100), 1.0);
    assert_eq!(perceptual_gain(50), 0.25);

    let (mut player, _) = NativePlayback::offline();
    player.set_volume(50).unwrap();
    let applied = f32::from_bits(player.shared.gain.load(Ordering::Acquire));
    assert_eq!(applied, 0.25);
    player.set_volume(0).unwrap();
    assert_eq!(
        f32::from_bits(player.shared.gain.load(Ordering::Acquire)),
        0.0
    );
    player.set_volume(200).unwrap();
    assert_eq!(
        f32::from_bits(player.shared.gain.load(Ordering::Acquire)),
        1.0,
        "gain clamps at unity and does not amplify"
    );
    player.set_volume(50).unwrap();

    let session = Arc::new(Session::new(
        PlaybackState::Playing,
        Timeline {
            anchor: Duration::ZERO,
            rate: 44_100,
            duration: None,
            failure: None,
        },
        true,
    ));
    let (mut producer, consumer) = RingBuffer::new(4);
    producer.push_entire_slice(&[1.0, -1.0, 0.5, -0.5]).unwrap();
    let mut source = PcmSource::new(
        Arc::clone(&session),
        Arc::new(DeviceFault::default()),
        consumer,
        1,
        44_100,
        Arc::clone(&player.shared.gain),
    );
    let actual: Vec<f32> = (0..4).map(|_| source.next().unwrap()).collect();
    for (got, expected) in actual.iter().zip([0.25, -0.25, 0.125, -0.125]) {
        assert!(
            (got - expected).abs() < 1.0e-6,
            "set 50 must apply gain 0.25, got {got} want {expected}"
        );
    }
}

#[test]
fn prepared_surround_audio_survives_status_lock_contention() {
    let expected = [0.25, -0.25, 0.5, -0.5, 0.75, -0.75];
    let session = Arc::new(Session::new(
        PlaybackState::Playing,
        Timeline {
            anchor: Duration::ZERO,
            rate: 44_100,
            duration: None,
            failure: None,
        },
        true,
    ));
    let (mut producer, consumer) = RingBuffer::new(expected.len());
    producer.push_entire_slice(&expected).unwrap();
    let mut source = PcmSource::new(
        Arc::clone(&session),
        Arc::new(DeviceFault::default()),
        consumer,
        6,
        44_100,
        unity_gain(),
    );
    let guard = lock(&session.timeline);
    let actual = std::array::from_fn::<_, 6, _>(|_| source.next().unwrap());
    drop(guard);
    assert_eq!(
        actual, expected,
        "prepared PCM must not be replaced with silence while another thread reads playback status"
    );
    assert_eq!(session.snapshot().1, 1);
    assert!(source.next().is_none());
    assert_eq!(session.state(), PlaybackState::Finished);
}

#[test]
fn concurrent_pause_freezes_submitted_audio_position() {
    let session = Arc::new(Session::new(
        PlaybackState::Playing,
        Timeline {
            anchor: Duration::ZERO,
            rate: 44_100,
            duration: None,
            failure: None,
        },
        false,
    ));
    let (mut producer, consumer) = RingBuffer::new(1_000_000);
    producer.push_entire_slice(&vec![0.25; 1_000_000]).unwrap();
    let mut source = PcmSource::new(
        Arc::clone(&session),
        Arc::new(DeviceFault::default()),
        consumer,
        1,
        44_100,
        unity_gain(),
    );
    let done = AtomicBool::new(false);
    let changed = thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                source.next();
            }
        });
        let mut changed = None;
        for _ in 0..10_000 {
            session.set_state(PlaybackState::Playing);
            thread::yield_now();
            session.set_state(PlaybackState::Paused);
            let before = lock(&session.timeline).status(&session).unwrap().position;
            thread::yield_now();
            let after = lock(&session.timeline).status(&session).unwrap().position;
            if after != before {
                changed = Some((before, after));
                break;
            }
        }
        done.store(true, Ordering::Release);
        changed
    });
    assert_eq!(
        changed, None,
        "a completed pause must freeze the submitted music position"
    );
}

fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "audio worker did not make progress"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn consume(
    player: &NativePlayback,
    samples: &mut rodio::mixer::MixerSource,
    until: impl Fn(&PlaybackStatus) -> bool,
) -> Vec<f32> {
    let mut output = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = player.status().unwrap();
        if until(&status) {
            return output;
        }
        assert!(
            Instant::now() < deadline,
            "output did not reach target: {status:?}"
        );
        for _ in 0..256 {
            output.extend(samples.next());
        }
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_mp3_and_flac_finish_and_replay() {
    for audio in [MP3, FLAC] {
        let (mut player, mut samples) = NativePlayback::offline();
        player.load(Box::new(Cursor::new(audio.to_vec()))).unwrap();
        let status = player.status().unwrap();
        assert_eq!(
            (status.state, status.position),
            (PlaybackState::Ready, Duration::ZERO)
        );
        player.play().unwrap();
        let pcm = consume(&player, &mut samples, |status| {
            status.state == PlaybackState::Finished
        });
        assert!(!pcm.is_empty());
        if audio == FLAC {
            assert!(pcm.iter().any(|sample| sample.abs() > 0.01));
        }
        player.play().unwrap();
        assert_eq!(player.status().unwrap().state, PlaybackState::Playing);
        player.stop().unwrap();
        assert_eq!(player.status().unwrap().state, PlaybackState::Ready);
        assert_eq!(player.status().unwrap().position, Duration::ZERO);
    }
}

#[test]
fn pause_seek_boundaries_and_stale_pcm_preserve_intent() {
    let (mut player, mut samples) = NativePlayback::offline();
    player.load(Box::new(Cursor::new(FLAC.to_vec()))).unwrap();
    let total = player.status().unwrap().duration.unwrap();
    player.play().unwrap();
    consume(&player, &mut samples, |status| {
        status.position >= Duration::from_millis(80)
    });
    player.pause().unwrap();
    let paused = player.status().unwrap().position;
    for _ in 0..10_000 {
        samples.next();
    }
    assert_eq!(player.status().unwrap().position, paused);
    let actual = player
        .seek(SeekTarget::Absolute(Duration::from_millis(500)))
        .unwrap();
    assert!(actual.abs_diff(Duration::from_millis(500)) < Duration::from_millis(2));
    assert_eq!(player.status().unwrap().state, PlaybackState::Paused);
    assert_eq!(
        player.seek(SeekTarget::Backward(Duration::MAX)).unwrap(),
        Duration::ZERO
    );
    assert_eq!(player.status().unwrap().state, PlaybackState::Paused);
    assert_eq!(
        player.seek(SeekTarget::Forward(Duration::MAX)).unwrap(),
        total
    );
    assert_eq!(player.status().unwrap().state, PlaybackState::Finished);
    player
        .seek(SeekTarget::Backward(Duration::from_millis(100)))
        .unwrap();
    assert_eq!(player.status().unwrap().state, PlaybackState::Paused);
    player.play().unwrap();
    consume(&player, &mut samples, |status| {
        status.state == PlaybackState::Finished
    });
    assert!(player.status().unwrap().position > Duration::from_millis(900));
    player.stop().unwrap();
    let ready_seek = player
        .seek(SeekTarget::Absolute(Duration::from_millis(200)))
        .unwrap();
    assert!(!ready_seek.is_zero());
    assert_eq!(player.status().unwrap().state, PlaybackState::Paused);
}

#[test]
fn bounded_preload_bad_candidate_and_playing_seek() {
    let (mut player, mut samples) = NativePlayback::offline();
    assert_eq!(player.play().unwrap_err().kind(), Kind::NoSource);
    assert_eq!(
        player
            .seek(SeekTarget::Forward(Duration::from_secs(1)))
            .unwrap_err()
            .kind(),
        Kind::NoSource
    );
    player.load(Box::new(Cursor::new(FLAC.to_vec()))).unwrap();
    let (output, _samples) = Output::offline();
    let mut engine = Engine {
        output,
        decoded: None,
        session: None,
        pcm: None,
        shared: Arc::new(Shared {
            session: Mutex::new(None),
            device: Mutex::new(Arc::new(DeviceFault::default())),
            alive: AtomicBool::new(true),
            gain: unity_gain(),
        }),
    };
    engine
        .control(Operation::Load(Box::new(Cursor::new(FLAC.to_vec()))))
        .unwrap();
    while engine.fill() {}
    let pcm = engine.pcm.as_ref().unwrap();
    assert_eq!(pcm.slots(), 0);
    assert_eq!(pcm.buffer().capacity(), 44_100 / 4);
    assert_eq!(player.status().unwrap().position, Duration::ZERO);
    player.play().unwrap();
    assert!(
        player
            .load(Box::new(Cursor::new(b"not audio".to_vec())))
            .is_err()
    );
    assert_eq!(player.status().unwrap().state, PlaybackState::Playing);
    let first = player
        .seek(SeekTarget::Forward(Duration::from_millis(100)))
        .unwrap();
    let second = player
        .seek(SeekTarget::Forward(Duration::from_millis(100)))
        .unwrap();
    assert!(second > first);
    assert_eq!(player.status().unwrap().state, PlaybackState::Playing);
    player.load(Box::new(Cursor::new(MP3.to_vec()))).unwrap();
    player.play().unwrap();
    consume(&player, &mut samples, |status| {
        status.state == PlaybackState::Finished
    });
    assert!(player.status().unwrap().position < Duration::from_millis(200));
}

#[test]
fn mp3_time_seek_returns_an_actual_music_position() {
    let (mut player, _) = NativePlayback::offline();
    player.load(Box::new(Cursor::new(MP3.to_vec()))).unwrap();
    let target = Duration::from_millis(30);
    let actual = player.seek(SeekTarget::Absolute(target)).unwrap();
    assert!(actual.abs_diff(target) < Duration::from_millis(2));
    assert_eq!(player.status().unwrap().state, PlaybackState::Paused);
    let later = player
        .seek(SeekTarget::Forward(Duration::from_millis(10)))
        .unwrap();
    assert!(later > actual);
    assert_eq!(
        player.seek(SeekTarget::Backward(Duration::MAX)).unwrap(),
        Duration::ZERO
    );
}

#[test]
fn corrupt_flac_is_a_persistent_decode_error_and_loading_recovers() {
    let (mut player, mut samples) = NativePlayback::offline();
    let mut corrupt = FLAC.to_vec();
    let offset = corrupt.len() - 100;
    corrupt[offset] ^= 0xff;
    player.load(Box::new(Cursor::new(corrupt))).unwrap();
    player.play().unwrap();
    wait(|| {
        for _ in 0..2_000 {
            samples.next();
        }
        player.status().is_err()
    });
    assert_eq!(player.status().unwrap_err().kind(), Kind::Decode);
    assert_eq!(player.status().unwrap_err().kind(), Kind::Decode);
    player.load(Box::new(Cursor::new(FLAC.to_vec()))).unwrap();
    assert_eq!(player.status().unwrap().state, PlaybackState::Ready);
}

#[test]
fn device_and_worker_failures_are_visible_and_stop_further_pcm() {
    let (mut player, mut samples) = NativePlayback::offline();
    player.load(Box::new(Cursor::new(FLAC.to_vec()))).unwrap();
    player.play().unwrap();
    let device = Arc::clone(&lock(&player.shared.device));
    device.set(PlaybackError::message(Kind::Output, "injected device loss"));
    assert_eq!(player.status().unwrap_err().kind(), Kind::Output);
    assert!(samples.by_ref().take(256).all(|sample| sample == 0.0));
    let (reply, _) = mpsc::channel();
    player
        .commands
        .send(Command {
            operation: Operation::Shutdown,
            reply,
        })
        .unwrap();
    wait(|| !player.shared.alive.load(Ordering::Acquire));
    assert_eq!(player.status().unwrap_err().kind(), Kind::Worker);
}

struct LocalReader {
    data: Cursor<Vec<u8>>,
    fail: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
    not_sync: Cell<usize>,
}

impl Read for LocalReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(io::Error::other("injected read failure"));
        }
        self.not_sync.set(self.not_sync.get() + 1);
        // Prevent the media stream from pre-reading the complete file.
        let count = bytes.len().min(32);
        self.data.read(&mut bytes[..count])
    }
}
impl Seek for LocalReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.data.seek(pos)
    }
}
impl Drop for LocalReader {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Relaxed);
    }
}

#[test]
fn non_sync_input_failure_is_sticky_and_stop_or_load_recovers() {
    let (mut player, mut samples) = NativePlayback::offline();
    let fail = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    player
        .load(Box::new(LocalReader {
            data: Cursor::new(FLAC.to_vec()),
            fail: Arc::clone(&fail),
            dropped: Arc::clone(&dropped),
            not_sync: Cell::new(0),
        }))
        .unwrap();
    player.play().unwrap();
    fail.store(true, Ordering::Relaxed);
    wait(|| {
        for _ in 0..2_000 {
            samples.next();
        }
        player.status().is_err()
    });
    assert_eq!(player.status().unwrap_err().kind(), Kind::Input);
    assert!(
        player
            .status()
            .unwrap_err()
            .to_string()
            .contains("injected read failure")
    );
    assert!(player.play().is_err());
    assert!(player.pause().is_err());
    assert!(player.stop().is_err());
    fail.store(false, Ordering::Relaxed);
    player.stop().unwrap();
    assert_eq!(player.status().unwrap().state, PlaybackState::Ready);
    assert!(!dropped.load(Ordering::Relaxed));
    player.load(Box::new(Cursor::new(MP3.to_vec()))).unwrap();
    assert!(dropped.load(Ordering::Relaxed));
}

#[test]
fn drop_releases_source_and_invalidates_output() {
    let (mut player, samples) = NativePlayback::offline();
    let dropped = Arc::new(AtomicBool::new(false));
    player
        .load(Box::new(LocalReader {
            data: Cursor::new(FLAC.to_vec()),
            fail: Arc::new(AtomicBool::new(false)),
            dropped: Arc::clone(&dropped),
            not_sync: Cell::new(0),
        }))
        .unwrap();
    player.play().unwrap();
    let shared = Arc::clone(&player.shared);
    drop(player);
    assert!(dropped.load(Ordering::Relaxed));
    assert!(!shared.alive.load(Ordering::Acquire));
    assert!(samples.take(256).all(|sample| sample == 0.0));
}

#[test]
#[ignore = "requires a physical default output device; run with --ignored --nocapture"]
fn default_device_smoke() {
    let mut player = NativePlayback::open_default().unwrap();
    player.load(Box::new(Cursor::new(FLAC.to_vec()))).unwrap();
    player.play().unwrap();
    thread::sleep(Duration::from_millis(120));
    assert!(player.status().unwrap().position > Duration::ZERO);
    player.pause().unwrap();
    let paused = player.status().unwrap().position;
    thread::sleep(Duration::from_millis(30));
    assert_eq!(player.status().unwrap().position, paused);
    player
        .seek(SeekTarget::Absolute(Duration::from_millis(500)))
        .unwrap();
    player.play().unwrap();
    wait(|| player.status().unwrap().state == PlaybackState::Finished);
    player.stop().unwrap();
    assert_eq!(player.status().unwrap().position, Duration::ZERO);
}
