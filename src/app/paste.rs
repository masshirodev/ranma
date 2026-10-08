//! The window manager's side of pasting files into a pane that runs ssh
//! (DESIGN.md, "Pasting files into a pane that runs ssh"): when a paste
//! uploads, the input held meanwhile, cancelling, and typing the answer. The
//! reading and uploading are `crate::paste`, on a thread of their own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use super::App;
use crate::input;
use crate::layout::PaneId;
use crate::nestbar;
use crate::pane::AppEvent;
use crate::paste::{self, Job, Source};

/// Input held while one is held back by an upload: more is a stuck paste,
/// not typing, and is dropped.
const HELD_MAX: usize = 4096;

/// How soon after a key passed to a ranma in a pane its `paste_image` request
/// must come: over ssh it is a round trip, never seconds.
const ASKED_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

/// A paste on its way: what it types into which pane, and the keys typed
/// meanwhile, sent after it so they cannot land first.
pub(super) struct PendingPaste {
    pub id: u64,
    pub pane: PaneId,
    /// Typed if the upload fails or is cancelled: the paste as it came.
    pub fallback: String,
    pub held: Vec<Event>,
    pub cancel: Arc<AtomicBool>,
}

impl App {
    /// A paste from the terminal. One that is nothing but paths of files
    /// here (a file dragged onto the terminal arrives this way), into a pane
    /// running ssh and nothing else, goes over first; the rest is typed.
    pub(super) fn paste(&mut self, text: String) {
        if self.config.settings.paste_upload
            && self.typing_targets().len() == 1
            && let Some(pane) = self.focused()
            && let Some(ssh) = self.focused_pane().and_then(|p| p.ssh_argv())
            && let Some(named) = paste::paths(&text, paste::wsl(), |p| p.is_file())
        {
            return self.start_paste(pane, Source::Files(named), Some(ssh), text);
        }
        self.typed(|modes| Some(input::encode_paste(&text, modes)));
    }

    /// `paste_image`: the files copied on the clipboard, else its image,
    /// uploaded when the pane runs ssh.
    /// Inside another ranma it is that one's to do, since the clipboard is on
    /// the machine at the keyboard: the path comes back as a paste.
    pub(super) fn paste_image(&mut self) {
        let Some(pane) = self.focused() else {
            self.status = Some("paste_image: no pane to paste into".into());
            return;
        };
        if self.bar_yielded() {
            self.host_out.push(nestbar::PASTE_IMAGE.as_bytes().to_vec());
            return;
        }
        self.paste_image_into(pane);
    }

    /// A ranma in pane `id` asked for `paste_image`. Heard only from a pane
    /// that runs a ranma and was typed into a moment ago: anything printing
    /// the sequence otherwise would be sent the clipboard's files.
    pub(super) fn paste_image_asked(&mut self, id: PaneId) {
        let recent = self
            .passed_key
            .is_some_and(|(p, at)| p == id && at.elapsed() < ASKED_WITHIN);
        if !recent || !self.panes.get(&id).is_some_and(|p| p.hosts_ranma()) {
            return;
        }
        // Still not the outermost: on up, and the path comes back down.
        if self.bar_yielded() {
            self.host_out.push(nestbar::PASTE_IMAGE.as_bytes().to_vec());
            return;
        }
        self.paste_image_into(id);
    }

    fn paste_image_into(&mut self, pane: PaneId) {
        let custom = self.config.settings.paste_image_command.as_deref();
        let Some(clip) = paste::clipboard(custom, paste::wsl(), |v| std::env::var(v).ok()) else {
            self.status =
                Some("paste_image: no WSL, Wayland or X11 here; set paste.image_command".into());
            return;
        };
        let ssh = self.panes.get(&pane).and_then(|p| p.ssh_argv());
        self.start_paste(pane, Source::Clipboard(clip), ssh, String::new());
    }

    fn start_paste(
        &mut self,
        pane: PaneId,
        source: Source,
        ssh: Option<Vec<String>>,
        fallback: String,
    ) {
        if self.pending_paste.is_some() {
            self.status = Some("a paste is still uploading (Esc cancels it)".into());
            return;
        }
        if let Some(host) = ssh
            .as_ref()
            .and_then(|a| crate::pane::ssh_destination(a.iter().skip(1).map(String::as_str)))
        {
            self.toast(
                format!("uploading to {host}…  Esc cancels"),
                crate::toast::Level::Normal,
                Some(std::time::Duration::from_secs(3)),
            );
        }
        self.paste_seq += 1;
        let id = self.paste_seq;
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            source,
            ssh,
            cancel: cancel.clone(),
        };
        self.pending_paste = Some(PendingPaste {
            id,
            pane,
            fallback,
            held: Vec::new(),
            cancel,
        });
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new()
            .name("paste".into())
            .spawn(move || {
                let result = paste::work(&job);
                let _ = tx.send(AppEvent::Pasted { id, result });
            });
    }

    /// While a paste uploads, keys and pastes wait for it, and Esc cancels
    /// it. Returns whether the event was taken.
    pub(super) fn hold_for_paste(&mut self, ev: &Event) -> bool {
        let Some(p) = self.pending_paste.as_mut() else {
            return false;
        };
        match ev {
            Event::Key(k) if k.kind == KeyEventKind::Release => true,
            Event::Key(k) if k.code == KeyCode::Esc && k.modifiers == KeyModifiers::NONE => {
                p.cancel.store(true, Ordering::Relaxed);
                self.finish_paste(Err("cancelled".into()));
                true
            }
            Event::Key(_) | Event::Paste(_) => {
                if p.held.len() < HELD_MAX {
                    p.held.push(ev.clone());
                }
                true
            }
            _ => false,
        }
    }

    /// The thread answered. An answer for a paste that was cancelled is late
    /// and dropped.
    pub(super) fn pasted(&mut self, id: u64, result: Result<String, String>) {
        if self.pending_paste.as_ref().is_some_and(|p| p.id == id) {
            self.finish_paste(result);
        }
    }

    /// Type the path (or the paste as it came, saying why), then what was
    /// typed meanwhile.
    fn finish_paste(&mut self, result: Result<String, String>) {
        let Some(p) = self.pending_paste.take() else {
            return;
        };
        let text = match result {
            Ok(path) => path,
            Err(why) => {
                self.toast(format!("paste: {why}"), crate::toast::Level::Urgent, None);
                p.fallback
            }
        };
        if !text.is_empty()
            && let Some(pane) = self.panes.get(&p.pane)
        {
            pane.scroll_to_bottom();
            pane.write(input::encode_paste(&text, pane.modes()));
        }
        self.dirty = true;
        for ev in p.held {
            self.handle_input(ev);
        }
    }

    /// Before an upgrade: an upload is not carried over, so its paste is typed
    /// as it came.
    pub(super) fn abandon_paste(&mut self) {
        if let Some(p) = &self.pending_paste {
            p.cancel.store(true, Ordering::Relaxed);
            self.finish_paste(Err("interrupted by an upgrade".into()));
        }
    }
}
