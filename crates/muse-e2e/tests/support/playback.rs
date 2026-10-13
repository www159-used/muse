// SPDX-License-Identifier: AGPL-3.0-only

use super::Result;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Session {
    child: Child,
    library: PathBuf,
    runtime: PathBuf,
    muse: PathBuf,
    log: PathBuf,
    advance_attempt: Option<std::result::Result<(), String>>,
}

impl Session {
    pub fn start(library: PathBuf, directory: &Path) -> Result<Self> {
        let muse = muse_binary()?;
        let runtime = directory.join("runtime");
        let log = directory.join("service.log");
        let log_file = fs::File::create(&log)?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .env("MUSE_E2E_CLOCK", "1")
            .env("MUSE_TEST_CHILD_LIBRARY", &library)
            .env("MUSE_RUNTIME_DIR", &runtime);
        // setsid first; that helper also clears stdio, so restore the log afterwards.
        crate::service::detached_process(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file.try_clone()?))
            .stderr(Stdio::from(log_file));
        let child = command.spawn()?;
        let mut session = Self {
            child,
            library,
            runtime,
            muse,
            log,
            advance_attempt: None,
        };
        session.wait_ready()?;
        Ok(session)
    }

    pub fn run(&mut self, args: &[&str]) -> Result<()> {
        let output = self.command(args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_failure(args, &output).into())
        }
    }

    pub fn attempt(&mut self, args: &[&str]) -> Result<()> {
        let output = self.command(args)?;
        self.advance_attempt = Some(if output.status.success() {
            Ok(())
        } else {
            Err(command_failure(args, &output))
        });
        Ok(())
    }

    pub fn expect_advance_failed(&self) -> Result<()> {
        match &self.advance_attempt {
            Some(Err(_)) => Ok(()),
            Some(Ok(())) => Err("next succeeded past the last track".into()),
            None => Err("next was not attempted".into()),
        }
    }

    pub fn status_line(&self, label: &str) -> Result<String> {
        let text = self.status_text()?;
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{label}: ")))
            .map(str::to_owned)
            .ok_or_else(|| format!("status has no {label} line:\n{text}").into())
    }

    fn status_text(&self) -> Result<String> {
        let output = self.command(&["status"])?;
        if !output.status.success() {
            return Err(command_failure(&["status"], &output).into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    fn wait_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last = String::new();
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait()? {
                return Err(format!(
                    "playback service exited ({status}): {}",
                    fs::read_to_string(&self.log).unwrap_or_default()
                )
                .into());
            }
            let output = self.command(&["status"])?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            if output.status.success() && stdout.contains("State:") {
                return Ok(());
            }
            last = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
            thread::sleep(Duration::from_millis(10));
        }
        Err(format!(
            "playback service did not become ready: {last}\n{}",
            fs::read_to_string(&self.log).unwrap_or_default()
        )
        .into())
    }

    fn command(&self, args: &[&str]) -> Result<Output> {
        Ok(Command::new(&self.muse)
            .arg("--library")
            .arg(&self.library)
            .args(args)
            .env("MUSE_RUNTIME_DIR", &self.runtime)
            .output()?)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn muse_binary() -> Result<PathBuf> {
    let mut path = std::env::current_exe()?;
    path.pop();
    path.pop();
    path.push(if cfg!(windows) { "muse.exe" } else { "muse" });
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "muse CLI is not built at {}; run make e2e or cargo build -p muse",
            path.display()
        )
        .into())
    }
}

fn command_failure(args: &[&str], output: &Output) -> String {
    format!(
        "muse {} failed ({}): {}{}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    )
}
