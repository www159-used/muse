// SPDX-License-Identifier: AGPL-3.0-only
use muse_library::{Library, SongGroup};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct Reader {
    pub updates: Receiver<Result<Vec<SongGroup>, String>>,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}
impl Reader {
    pub fn start(path: PathBuf) -> Self {
        let (sender, updates) = mpsc::channel();
        let (stop, quit) = mpsc::channel();
        let thread = thread::spawn(move || {
            let mut library = None;
            let mut previous = None;
            loop {
                let result = (|| {
                    if library.is_none() {
                        library = Some(Library::open_existing(&path).map_err(|e| e.to_string())?);
                        previous = None;
                    }
                    let lib = library.as_ref().unwrap();
                    // Read before snapshot: commits racing this read are seen next iteration.
                    let version = lib.data_version().map_err(|e| e.to_string())?;
                    if previous != Some(version) {
                        let groups = lib.groups().map_err(|e| e.to_string())?;
                        previous = Some(version);
                        Ok(Some(groups))
                    } else {
                        Ok(None)
                    }
                })();
                match result {
                    Ok(Some(groups)) => {
                        if sender.send(Ok(groups)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        library = None;
                        if sender.send(Err(e)).is_err() {
                            break;
                        }
                    }
                }
                if quit.recv_timeout(Duration::from_millis(500))
                    != Err(mpsc::RecvTimeoutError::Timeout)
                {
                    break;
                }
            }
        });
        Self {
            updates,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_recovers_from_missing_library_and_only_publishes_commits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.muse");
        let reader = Reader::start(path.clone());
        assert!(
            reader
                .updates
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .is_err()
        );
        let mut library = Library::open(&path).unwrap();
        assert!(
            reader
                .updates
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            reader.updates.recv_timeout(Duration::from_millis(650)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        let source = dir.path().join("song.mp3");
        std::fs::write(
            &source,
            include_bytes!("../../muse-library/tests/fixtures/silence.mp3"),
        )
        .unwrap();
        assert_eq!(library.import(&source).imported, 1);
        let groups = reader
            .updates
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(groups.len(), 1);
        library.remove(groups[0].candidates[0].id).unwrap();
        assert!(
            reader
                .updates
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap()
                .is_empty()
        );
    }
}
