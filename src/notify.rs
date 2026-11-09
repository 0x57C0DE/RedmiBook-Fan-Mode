//! Desktop banners in every logged-in session.
//!
//! The daemon runs as root, but a session bus only accepts its own user. For
//! each banner the daemon re-executes itself as that user (`notify`
//! subcommand), which sends the notification over D-Bus, prints its id, keeps
//! it on screen for a moment and then closes it. Closing matters: GNOME keeps
//! even transient notifications in the list under the clock otherwise.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::Value;

use crate::mode::Mode;

const APP_NAME: &str = "Fan mode";
const SUMMARY: &str = "Fan mode";
const URGENCY_NORMAL: u8 = 1;
const SPAWN_TIMEOUT: Duration = Duration::from_secs(3);

/// How long a banner stays before it is closed.
pub const BANNER_TIME: Duration = Duration::from_secs(3);

/// A graphical session found through its bus socket in `/run/user/<uid>`.
struct Session {
    uid: u32,
    gid: u32,
    runtime_dir: PathBuf,
}

fn sessions() -> Vec<Session> {
    let Ok(entries) = fs::read_dir("/run/user") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let runtime_dir = entry.ok()?.path();
            let uid: u32 = runtime_dir.file_name()?.to_str()?.parse().ok()?;
            if uid < 1000 || !runtime_dir.join("bus").exists() {
                return None;
            }
            // The runtime dir is owned by the user and their primary group.
            let gid = fs::metadata(&runtime_dir).ok()?.gid();
            Some(Session {
                uid,
                gid,
                runtime_dir,
            })
        })
        .collect()
}

/// A banner helper process and the id of the notification it shows.
struct Banner {
    child: Child,
    id: u32,
}

pub struct Notifier {
    exe: PathBuf,
    banners: HashMap<u32, Banner>,
}

impl Notifier {
    pub fn new() -> Result<Self> {
        let exe = std::env::current_exe().context("locating own executable")?;
        Ok(Self {
            exe,
            banners: HashMap::new(),
        })
    }

    /// Shows `mode` in every session, replacing a banner that is still on screen.
    pub fn show(&mut self, mode: Mode) {
        for session in sessions() {
            let replaces = self.retire(session.uid);
            match self.spawn(&session, mode, replaces) {
                Ok(banner) => {
                    self.banners.insert(session.uid, banner);
                }
                Err(err) => eprintln!("banner for uid {}: {err:#}", session.uid),
            }
        }
    }

    /// Stops the previous helper of this user. Returns the id to replace, or 0
    /// when that banner has already been closed.
    fn retire(&mut self, uid: u32) -> u32 {
        let Some(mut banner) = self.banners.remove(&uid) else {
            return 0;
        };
        if matches!(banner.child.try_wait(), Ok(Some(_))) {
            return 0;
        }
        let _ = banner.child.kill();
        let _ = banner.child.wait();
        banner.id
    }

    fn spawn(&self, session: &Session, mode: Mode, replaces: u32) -> Result<Banner> {
        let bus = session.runtime_dir.join("bus");
        let mut child = Command::new(&self.exe)
            .arg("notify")
            .args(["--mode", &mode.raw().to_string()])
            .args(["--replaces", &replaces.to_string()])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_RUNTIME_DIR", &session.runtime_dir)
            .env("DBUS_SESSION_BUS_ADDRESS", bus_address(&bus))
            // std drops supplementary groups when switching away from root.
            .uid(session.uid)
            .gid(session.gid)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .context("starting banner helper")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("no helper stdout"))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = tx.send(line);
        });
        let line = match rx.recv_timeout(SPAWN_TIMEOUT) {
            Ok(line) => line,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("helper did not answer within {SPAWN_TIMEOUT:?}"));
            }
        };
        match line.trim().parse() {
            Ok(id) => Ok(Banner { child, id }),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(anyhow!("helper failed, see its error above"))
            }
        }
    }
}

fn bus_address(path: &Path) -> String {
    format!("unix:path={}", path.display())
}

/// Body of the `notify` subcommand, run as the session user: shows the banner,
/// prints its id for the daemon, waits and closes it.
pub fn run_banner(mode: Mode, replaces: u32, linger: Duration) -> Result<()> {
    let conn = Connection::session().context("connecting to the session bus")?;
    let proxy = Proxy::new(
        &conn,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("urgency", Value::from(URGENCY_NORMAL));
    hints.insert("transient", Value::from(true));
    let actions: Vec<&str> = Vec::new();
    let expire_ms = i32::try_from(linger.as_millis()).unwrap_or(i32::MAX);
    let body = mode.to_string();

    let id: u32 = proxy
        .call(
            "Notify",
            &(
                APP_NAME,
                replaces,
                mode.icon(),
                SUMMARY,
                body.as_str(),
                actions,
                hints,
                expire_ms,
            ),
        )
        .context("sending the notification")?;
    println!("{id}");
    std::io::Write::flush(&mut std::io::stdout())?;

    thread::sleep(linger);
    let () = proxy
        .call("CloseNotification", &(id,))
        .context("closing the notification")?;
    Ok(())
}
