//! The window manager's side of opening a far file here (DESIGN.md,
//! "Opening a far file here"): `ranma.open_here`, the request a ranma in a
//! pane sends, and passing it on up. The fetching is `crate::fetch`, on a
//! thread of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::App;
use crate::fetch::{self, Job};
use crate::layout::PaneId;
use crate::nestbar;
use crate::pane::AppEvent;
use crate::toast::Level;

/// How soon after a key or a click passed to a ranma in a pane its request
/// must come. Longer than `paste_image`'s, since a ranma in the middle of a
/// chain fetches the file before it asks on up.
const ASKED_WITHIN: Duration = Duration::from_secs(35);

/// A file on its way: what to do with it when it is here.
pub(super) struct PendingOpen {
    pub id: u64,
    /// Ask the ranma around this one to open it, rather than opening it here.
    pub forward: bool,
    pub cancel: Arc<AtomicBool>,
}

impl App {
    /// `ranma.open_here(path)`: a file of this machine, opened on the machine
    /// at the keyboard. Inside another ranma that is the outermost one's to
    /// do, and it fetches the file first.
    pub(super) fn open_here(&mut self, path: String) {
        if self.bar_yielded() {
            self.ask_outer_to_open(&path);
            return;
        }
        if self.client_remote {
            self.toast(
                "open_here: this terminal came over ssh with no ranma around it, so there is no screen here to open it on",
                Level::Urgent,
                None,
            );
            return;
        }
        self.start_open(path, None, false);
    }

    fn ask_outer_to_open(&mut self, path: &str) {
        if self.outer_v >= nestbar::OPEN_HERE_SINCE {
            self.host_out.push(nestbar::open_here_osc(path));
        } else {
            self.toast(
                "open_here: the ranma around this one is too old to open files; upgrade it",
                Level::Urgent,
                None,
            );
        }
    }

    /// A ranma in pane `id` asks to open a file of its machine. Heard only
    /// from a pane that runs a ranma and was typed or clicked into a moment
    /// ago: anything else printing the sequence would have files opened here.
    pub(super) fn open_here_asked(&mut self, id: PaneId, path: String) {
        let recent = self
            .passed_key
            .is_some_and(|(p, at)| p == id && at.elapsed() < ASKED_WITHIN);
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        if !recent || !pane.hosts_ranma() {
            return;
        }
        let ssh = pane.ssh_argv();
        // Still not the outermost: fetch it here, then ask on up with the
        // path here, as an upload chains the other way.
        self.start_open(path, ssh, self.bar_yielded());
    }

    fn start_open(&mut self, path: String, ssh: Option<Vec<String>>, forward: bool) {
        if let Some(p) = &self.pending_open {
            p.cancel.store(true, Ordering::Relaxed);
        }
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        if let Some(host) = ssh
            .as_ref()
            .and_then(|a| crate::pane::ssh_destination(a.iter().skip(1).map(String::as_str)))
        {
            self.toast(
                format!("fetching {name} from {host}…"),
                Level::Normal,
                Some(Duration::from_secs(3)),
            );
        }
        self.open_seq += 1;
        let id = self.open_seq;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            path,
            ssh,
            open: !forward,
            cancel: cancel.clone(),
        };
        self.pending_open = Some(PendingOpen {
            id,
            forward,
            cancel,
        });
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new()
            .name("open-here".into())
            .spawn(move || {
                let result = fetch::work(&job);
                let _ = tx.send(AppEvent::Fetched { id, result });
            });
    }

    /// The thread answered. An answer for a file another one replaced is
    /// dropped.
    pub(super) fn fetched(&mut self, id: u64, result: Result<std::path::PathBuf, String>) {
        if !self.pending_open.as_ref().is_some_and(|p| p.id == id) {
            return;
        }
        let Some(p) = self.pending_open.take() else {
            return;
        };
        match result {
            Err(why) => self.toast(format!("open_here: {why}"), Level::Urgent, None),
            Ok(here) if p.forward => {
                if self.bar_yielded() {
                    self.ask_outer_to_open(&here.to_string_lossy());
                }
            }
            Ok(_) => {}
        }
        self.dirty = true;
    }
}
