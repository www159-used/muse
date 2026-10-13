// SPDX-License-Identifier: AGPL-3.0-only

use super::playback::Session;
use muse_library::{ImportReport, Library};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
};
use tempfile::TempDir;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(cucumber::World)]
#[world(init = Self::new)]
pub struct ImportWorld {
    playback: Option<Session>,
    // Drop the connection before removing its directory on Windows.
    pub library: Library,
    pub last_report: Option<ImportReport>,
    failed_name: Option<String>,
    source: PathBuf,
    legacy_path: Option<PathBuf>,
    originals: BTreeMap<PathBuf, Vec<u8>>,
    _temporary: TempDir,
}

impl fmt::Debug for ImportWorld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportWorld")
            .field("source", &self.source)
            .field("last_report", &self.last_report)
            .finish_non_exhaustive()
    }
}

impl ImportWorld {
    fn new() -> std::io::Result<Self> {
        let temporary = TempDir::new()?;
        let source = temporary.path().join("sources");
        fs::create_dir(&source)?;
        Ok(Self {
            playback: None,
            library: Library::open(temporary.path().join("library.muse"))
                .map_err(std::io::Error::other)?,
            last_report: None,
            failed_name: None,
            legacy_path: None,
            source,
            originals: BTreeMap::new(),
            _temporary: temporary,
        })
    }

    pub fn copy_ncm(&mut self, relative: &str, fixture: &str) -> Result<()> {
        let path = self.source_path(relative)?;
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ncm"))
        {
            return Err("the source file must have an NCM extension".into());
        }
        self.copy_fixture(relative, fixture)
    }

    pub fn copy_fixture(&mut self, relative: &str, fixture: &str) -> Result<()> {
        self.write_source(relative, Self::fixture(fixture)?)
    }

    pub fn write_corrupt(&mut self, relative: &str) -> Result<()> {
        self.write_source(relative, b"muse-corrupt-audio".to_vec())
    }

    pub fn replace_fixture(&mut self, relative: &str, fixture: &str) -> Result<()> {
        let path = self.source_path(relative)?;
        if !path.is_file() {
            return Err(format!("source does not exist: {relative}").into());
        }
        let bytes = Self::fixture(fixture)?;
        fs::write(&path, &bytes)?;
        self.originals.insert(path, bytes);
        Ok(())
    }

    pub fn empty_folder(&mut self, relative: &str) -> Result<()> {
        let path = self.source_path(relative)?;
        if path.exists() {
            return Err(format!("folder already exists: {relative}").into());
        }
        fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn fail_import_on(&mut self, relative: &str) -> Result<()> {
        self.write_corrupt(relative)?;
        let folder = Path::new(relative)
            .parent()
            .and_then(|parent| parent.to_str())
            .filter(|parent| !parent.is_empty())
            .ok_or("failed import needs a file inside a folder")?;
        self.import_folder(folder)?;
        let name = Path::new(relative)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("failed import needs a file name")?;
        let failed = self
            .report()?
            .failed
            .iter()
            .any(|issue| issue.path.file_name().and_then(|file| file.to_str()) == Some(name));
        if !failed {
            return Err(format!("import did not fail on {name}").into());
        }
        self.failed_name = Some(name.to_owned());
        Ok(())
    }

    pub fn import_folder(&mut self, folder: &str) -> Result<()> {
        let path = self.source_path(folder)?;
        if !path.is_dir() {
            return Err(format!("download folder does not exist: {folder}").into());
        }
        self.last_report = Some(self.library.import_downloads_with_progress(
            &path,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        ));
        self.check_sources()
    }

    pub fn track_for_fixture(&self, fixture: &str) -> Result<muse_library::Track> {
        let bytes = Self::fixture(fixture)?;
        let mut matches = Vec::new();
        for track in self.library.list()? {
            if self.library.open_audio(track.id)?.into_inner() == bytes {
                matches.push(track);
            }
        }
        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err(format!("no track stores fixture {fixture}").into()),
            _ => Err(format!("fixture {fixture} matches multiple tracks").into()),
        }
    }

    pub fn queue_fixture_next(&mut self, fixture: &str) -> Result<()> {
        let id = self.track_for_fixture(fixture)?.id;
        self.session()?.run(&["next-once", &id.to_string()])
    }

    pub fn failed_name(&self) -> Result<&str> {
        self.failed_name
            .as_deref()
            .ok_or_else(|| "no previous import failure was recorded".into())
    }

    pub fn write_source(&mut self, relative: &str, bytes: Vec<u8>) -> Result<()> {
        let path = self.source_path(relative)?;
        if path.exists() {
            return Err(format!("source already exists: {relative}").into());
        }
        fs::create_dir_all(path.parent().ok_or("source file has no parent")?)?;
        fs::write(&path, &bytes)?;
        self.originals.insert(path, bytes);
        Ok(())
    }

    pub fn source_path(&self, relative: &str) -> Result<PathBuf> {
        safe_path(&self.source, relative)
    }

    pub fn reopen_library(&mut self) -> Result<()> {
        self.library = Library::open(
            self.legacy_path
                .clone()
                .unwrap_or_else(|| self._temporary.path().join("library.muse")),
        )?;
        Ok(())
    }

    pub fn prepare_legacy(&mut self, source: &str) -> Result<()> {
        use sha2::{Digest, Sha256};
        let path = self._temporary.path().join("legacy.muse");
        let db = rusqlite::Connection::open(&path)?;
        db.execute_batch(include_str!("../../../muse-library/src/schema.sql"))?;
        db.execute_batch("PRAGMA user_version=1; PRAGMA journal_mode=WAL;")?;
        let audio = Self::fixture("same-song.mp3")?;
        let hash = format!("{:x}", Sha256::digest(&audio));
        db.execute(
            "INSERT INTO audio VALUES(?1,?2)",
            rusqlite::params![hash, audio],
        )?;
        db.execute("INSERT INTO tracks VALUES(42,'Fixture NCM','Fixture Artist','Fixture Album','mp3',NULL,NULL,?1)",[hash])?;
        let source_hash = format!("{:x}", Sha256::digest(fs::read(self.source_path(source)?)?));
        db.execute("INSERT INTO import_sources VALUES(?1,42)", [source_hash])?;
        self.legacy_path = Some(path);
        Ok(())
    }

    pub fn upgrade_legacy(&mut self) -> Result<()> {
        let path = self
            .legacy_path
            .as_ref()
            .ok_or("legacy fixture is missing")?;
        let backup = Library::upgrade(path)?.ok_or("expected schema upgrade")?;
        assert!(backup.is_file());
        self.library = Library::open(path)?;
        assert_eq!(self.library.list()?[0].id, 42);
        Ok(())
    }

    pub fn fixture(name: &str) -> Result<Vec<u8>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let path = safe_path(&root, name)?.canonicalize()?;
        if !path.starts_with(root.canonicalize()?) {
            return Err(format!("fixture escapes its root: {name}").into());
        }
        Ok(fs::read(path)?)
    }

    pub fn check_sources(&self) -> Result<()> {
        for (path, bytes) in &self.originals {
            assert_eq!(
                fs::read(path)?,
                *bytes,
                "source changed: {}",
                path.display()
            );
        }
        Ok(())
    }

    pub fn play_playlist(&mut self, name: &str) -> Result<()> {
        self.session()?.run(&["playlist", "play", name])
    }

    pub fn play_fixture(&mut self, fixture: &str) -> Result<()> {
        let id = self.track_for_fixture(fixture)?.id;
        self.session()?.run(&["play", &id.to_string()])
    }

    pub fn advance(&mut self) -> Result<()> {
        self.session()?.run(&["next"])
    }

    pub fn attempt_advance(&mut self) -> Result<()> {
        self.session()?.attempt(&["next"])
    }

    pub fn cycle_mode(&mut self) -> Result<()> {
        self.session()?.run(&["mode"])
    }

    pub fn expect_advance_failed(&self) -> Result<()> {
        self.playback
            .as_ref()
            .ok_or("playback has not started")?
            .expect_advance_failed()
    }

    pub fn status_line(&self, label: &str) -> Result<String> {
        self.playback
            .as_ref()
            .ok_or("playback has not started")?
            .status_line(label)
    }

    fn library_path(&self) -> PathBuf {
        self.legacy_path
            .clone()
            .unwrap_or_else(|| self._temporary.path().join("library.muse"))
    }

    fn session(&mut self) -> Result<&mut Session> {
        if self.playback.is_none() {
            self.playback = Some(Session::start(self.library_path(), self._temporary.path())?);
        }
        Ok(self.playback.as_mut().unwrap())
    }

    pub fn report(&self) -> Result<&ImportReport> {
        self.last_report
            .as_ref()
            .ok_or_else(|| "import a folder before checking its result".into())
    }
}

fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("path must stay inside the sandbox: {relative}").into());
    }
    Ok(root.join(relative))
}
