// SPDX-License-Identifier: AGPL-3.0-only

use super::protocol::VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct VolumeFile {
    volume: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub version: u32,
    pub port: u16,
    pub token: String,
    pub library: String,
}

pub struct Runtime {
    pub library: PathBuf,
    pub identity: String,
    pub root: PathBuf,
}

impl Runtime {
    pub fn existing(path: &Path) -> Result<Option<Self>, String> {
        let library = match path.canonicalize() {
            Ok(path) => path,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let identity = format!(
            "{:x}",
            Sha256::digest(library.as_os_str().as_encoded_bytes())
        );
        let root = if let Some(path) = std::env::var_os("MUSE_RUNTIME_DIR") {
            PathBuf::from(path)
        } else {
            #[cfg(unix)]
            let home = std::env::var_os("HOME");
            #[cfg(windows)]
            let home = std::env::var_os("LOCALAPPDATA");
            PathBuf::from(home.ok_or("cannot locate the user's runtime directory")?)
                .join(".muse-sessions")
        };
        Ok(Some(Self {
            library,
            identity,
            root,
        }))
    }

    pub fn endpoint_path(&self) -> PathBuf {
        self.root.join(format!("{}.json", self.identity))
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join(format!("{}.settings.json", self.identity))
    }

    /// Loads the persisted UI volume. A missing file is the daemon default of
    /// 100. This is not a reading of a live service.
    pub fn load_volume(&self) -> Result<u8, String> {
        match fs::read(self.settings_path()) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(100),
            Err(error) => Err(format!("cannot read volume settings: {error}")),
            Ok(bytes) => {
                if bytes.len() > 4096 {
                    return Err("volume settings file is too large".into());
                }
                let stored: VolumeFile = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("cannot read volume settings: {error}"))?;
                if stored.volume > 100 {
                    return Err(format!(
                        "stored volume {} is out of range; expected an integer from 0 to 100",
                        stored.volume
                    ));
                }
                Ok(stored.volume)
            }
        }
    }

    pub fn store_volume(&self, volume: u8) -> Result<(), String> {
        if volume > 100 {
            return Err(format!(
                "volume {volume} is out of range; use an integer from 0 to 100"
            ));
        }
        self.prepare()?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &VolumeFile { volume }).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        let path = self.settings_path();
        file.persist(&path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn lock_path(&self) -> PathBuf {
        self.root.join(format!("{}.lock", self.identity))
    }

    pub fn prepare(&self) -> Result<(), String> {
        private_directory(&self.root)
            .map_err(|e| format!("cannot prepare service runtime directory: {e}"))
    }

    pub fn lock(&self, create: bool) -> Result<Option<File>, String> {
        if create {
            self.prepare()?;
        } else if !self.root.exists() {
            return Ok(None);
        }
        check_directory(&self.root).map_err(|e| e.to_string())?;
        let mut options = File::options();
        options
            .read(true)
            .write(true)
            .create(create)
            .truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(self.lock_path()) {
            Ok(file) => Ok(Some(file)),
            Err(e) if !create && e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn endpoint(&self) -> Result<Endpoint, String> {
        check_directory(&self.root).map_err(|e| e.to_string())?;
        let file = File::open(self.endpoint_path()).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 4096 {
            return Err("invalid service endpoint".into());
        }
        let endpoint: Endpoint = serde_json::from_reader(file)
            .map_err(|e| format!("cannot read service endpoint: {e}"))?;
        if endpoint.version != VERSION {
            return Err("incompatible playback service; shut it down with its original muse binary before upgrading".into());
        }
        if endpoint.library != self.identity {
            return Err("service library identity mismatch".into());
        }
        Ok(endpoint)
    }

    pub fn publish(&self, port: u16) -> Result<Endpoint, String> {
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let endpoint = Endpoint {
            version: VERSION,
            port,
            token: random.iter().map(|b| format!("{b:02x}")).collect(),
            library: self.identity.clone(),
        };
        let mut file = tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &endpoint).map_err(|e| e.to_string())?;
        file.persist(self.endpoint_path())
            .map_err(|e| e.to_string())?;
        Ok(endpoint)
    }
}

#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    check_directory(path)
}

#[cfg(unix)]
fn check_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::other(
            "service runtime directory must be owned by this user, mode 0700, and not a symlink",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn private_directory(path: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::CreateDirectoryW,
    };
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // A protected DACL gives only the object's owner access, inherited by children.
    let sddl: Vec<u16> = "D:P(A;OICI;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor = ptr::null_mut();
    // SAFETY: pointers refer to live, NUL-terminated buffers and an output pointer.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let created = CreateDirectoryW(name.as_ptr(), &attributes);
        let error = io::Error::last_os_error();
        LocalFree(descriptor);
        if created == 0 && error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    check_directory(path)
}

#[cfg(windows)]
fn check_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other(
            "service runtime directory must not be a reparse point",
        ));
    }
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
                SE_FILE_OBJECT,
            },
            DACL_SECURITY_INFORMATION,
        },
    };
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    let mut text = ptr::null_mut();
    let mut length = 0;
    // SAFETY: the API allocates both buffers, which are read only within their reported
    // bounds and freed with LocalFree. No descriptor pointers escape this function.
    unsafe {
        let error = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        );
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            &mut length,
        );
        let error = io::Error::last_os_error();
        let private = converted != 0
            && String::from_utf16_lossy(std::slice::from_raw_parts(text, length as usize))
                .trim_end_matches('\0')
                == "D:P(A;OICI;FA;;;OW)";
        if !text.is_null() {
            LocalFree(text.cast());
        }
        LocalFree(descriptor);
        if converted == 0 {
            return Err(error);
        }
        if !private {
            return Err(io::Error::other(
                "service runtime directory requires a protected owner-only DACL",
            ));
        }
    }
    Ok(())
}
