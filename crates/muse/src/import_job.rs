// SPDX-License-Identifier: AGPL-3.0-only
use muse_library::{ImportProgress, ImportReport, Library};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
pub(crate) struct ImportWorker {
    pub(crate) source: PathBuf,
    pub(crate) progress: Arc<Mutex<Option<ImportProgress>>>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) result: Receiver<Result<ImportReport, String>>,
    pub(crate) thread: Option<JoinHandle<()>>,
}

impl ImportWorker {
    pub(crate) fn start(library: PathBuf, source: PathBuf) -> Self {
        let progress = Arc::new(Mutex::new(None));
        let worker_progress = Arc::clone(&progress);
        let worker_source = source.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, result) = mpsc::channel();
        let thread = thread::spawn(move || {
            let result = Library::open(library)
                .map(|mut library| {
                    library.import_downloads_with_progress(
                        worker_source,
                        &worker_cancel,
                        |snapshot| {
                            *worker_progress
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(snapshot);
                        },
                    )
                })
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        Self {
            source,
            progress,
            cancel,
            result,
            thread: Some(thread),
        }
    }

    pub(crate) fn snapshot(&self) -> Option<ImportProgress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for ImportWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
