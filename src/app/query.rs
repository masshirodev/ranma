//! Answering queries from the socket: `ranma open`, `panes`, `send`,
//! `capture` and `wait` (see `ipc`). Each gets a reply on its own channel; the
//! connection's thread is waiting on it.

use std::sync::mpsc::Sender;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;

use super::{App, SCRATCHPAD, chord_bytes};
use crate::input;
use crate::ipc::{PaneInfo, PaneOp, Query, SendInput};
use crate::layout::PaneId;
use crate::workspace::Workspace;

pub type Reply = Sender<Result<String, String>>;

/// How many ended panes' exit statuses `wait` remembers.
const ENDED_KEPT: usize = 64;

impl App {
    pub(super) fn answer(&mut self, q: Query, reply: Reply) {
        let result = match q {
            Query::Open(spec) => self.open_spec(spec).map(|id| format!("{id}\n")),
            Query::Panes => serde_json::to_string(&self.pane_infos())
                .map(|j| j + "\n")
                .map_err(|e| e.to_string()),
            Query::Send { pane, input } => self.send_input(pane, &input).map(|_| String::new()),
            Query::Capture { pane, history } => self.capture(pane, history),
            Query::Pane { pane, op } => self.pane_op(pane, op).map(|_| String::new()),
            Query::Wait { pane } => {
                if self.panes.contains_key(&pane) {
                    self.waiters.entry(pane).or_default().push(reply);
                    return;
                }
                // Ids are never reused: one below the next is a pane that has
                // already ended, which a script racing its own pane will ask about.
                if pane > 0 && pane < self.next_id {
                    Ok(self.ended_status(pane))
                } else {
                    Err(no_pane(pane))
                }
            }
        };
        let _ = reply.send(result);
    }

    /// A pane is gone: whoever waits on it gets its exit status, and it is
    /// kept a while for whoever asks too late.
    pub(super) fn pane_ended(&mut self, id: PaneId) {
        let code = self.exit_codes.remove(&id);
        self.ended.push_back((id, code));
        if self.ended.len() > ENDED_KEPT {
            self.ended.pop_front();
        }
        let status = self.ended_status(id);
        for w in self.waiters.remove(&id).unwrap_or_default() {
            let _ = w.send(Ok(status.clone()));
        }
    }

    /// What `wait` answers for a pane that has ended: its exit status, or
    /// nothing when it had none or ended too long ago to remember.
    fn ended_status(&self, id: PaneId) -> String {
        self.ended
            .iter()
            .find(|(p, _)| *p == id)
            .and_then(|(_, c)| *c)
            .map(|c| format!("{c}\n"))
            .unwrap_or_default()
    }

    /// The session and workspace holding a pane, shown or not.
    fn place_of(&self, id: PaneId) -> Option<(String, u8, &Workspace)> {
        if self.scratch.contains(id) {
            return Some((self.session_name().to_string(), SCRATCHPAD, &self.scratch));
        }
        if let Some((n, ws)) = self.workspaces.iter().find(|(_, ws)| ws.contains(id)) {
            return Some((self.session_name().to_string(), *n, ws));
        }
        let (si, n) = self.locate_hidden(id)?;
        let s = &self.sessions[si];
        Some((s.name.clone(), n, s.workspaces.get(&n)?))
    }

    pub(super) fn pane_infos(&self) -> Vec<PaneInfo> {
        let mut ids: Vec<PaneId> = self.panes.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| {
                let p = self.panes.get(&id)?;
                let (session, workspace, ws) = self.place_of(id)?;
                let shown = session == self.session_name()
                    && if workspace == SCRATCHPAD {
                        self.scratch_shown
                    } else {
                        !self.scratch_shown && workspace == self.current
                    };
                Some(PaneInfo {
                    id,
                    session,
                    workspace,
                    focused: ws.focused == Some(id),
                    visible: self.visible.contains(&id),
                    floating: ws.is_floating(id),
                    title: p.label().to_string(),
                    program: p.program(),
                    cwd: p.cwd(),
                    pid: p.pid,
                    cols: p.size.cols,
                    rows: p.size.rows,
                    workspace_name: ws.name.clone(),
                    workspace_shown: shown,
                })
            })
            .collect()
    }

    pub(super) fn pane_op(&mut self, id: PaneId, op: PaneOp) -> Result<(), String> {
        if !self.panes.contains_key(&id) {
            return Err(no_pane(id));
        }
        match op {
            PaneOp::Close => self.close_pane(id),
            PaneOp::Rename(name) => self.rename_pane(id, &name),
            PaneOp::Focus => {
                if let Some((si, _)) = self.locate_hidden(id) {
                    self.switch_session(si);
                }
                match self.locate(id) {
                    Some(SCRATCHPAD) => self.scratch_shown = true,
                    Some(n) => self.switch_workspace(n),
                    None => return Err(no_pane(id)),
                }
                self.focus(id);
                self.relayout();
            }
            PaneOp::Respawn { command, cwd } => self.respawn(id, command.as_deref(), cwd)?,
        }
        Ok(())
    }

    /// Replace a pane's process, keeping the pane: its place, size, id and
    /// name. The old process is hung up as its PTY closes; its events are
    /// muted first, so its exit does not close the pane it used to be in.
    fn respawn(
        &mut self,
        id: PaneId,
        command: Option<&str>,
        cwd: Option<std::path::PathBuf>,
    ) -> Result<(), String> {
        let old = self.panes.get(&id).ok_or_else(|| no_pane(id))?;
        let size = old.size;
        let cwd = cwd.or_else(|| old.cwd());
        let name = old.name.clone();
        let s = &self.config.settings;
        let opts = crate::pane::SpawnOptions {
            shell: s.shell.as_deref(),
            command,
            scrollback_lines: s.scrollback_lines,
            cwd,
        };
        let mut new = crate::pane::Pane::spawn(id, size, &opts, self.tx.clone())
            .map_err(|e| format!("respawning pane {id}: {e:#}"))?;
        new.name = name;
        if let Some(old) = self.panes.insert(id, new) {
            old.retire();
        }
        self.exit_codes.remove(&id);
        self.rules_applied.retain(|(p, _)| *p != id);
        if let Some(cmd) = command {
            self.apply_command_rules(id, cmd);
        }
        self.dirty = true;
        Ok(())
    }

    /// Input for a pane as if typed there. A newline in text is Enter (`\r`),
    /// which is what a terminal sends for the key.
    pub(super) fn send_input(&mut self, id: PaneId, input: &SendInput) -> Result<(), String> {
        let p = self.panes.get(&id).ok_or_else(|| no_pane(id))?;
        let modes = p.modes();
        let bytes = match input {
            SendInput::Text(t) => t.replace("\r\n", "\r").replace('\n', "\r").into_bytes(),
            SendInput::Paste(t) => input::encode_paste(t, modes),
            SendInput::Keys(keys) => {
                let mut out = Vec::new();
                for k in keys {
                    out.extend(
                        chord_bytes(*k, modes)
                            .ok_or_else(|| format!("`{k}` has no bytes to send"))?,
                    );
                }
                out
            }
        };
        p.scroll_to_bottom();
        p.write(bytes);
        Ok(())
    }

    /// A pane's text, `history` lines of scrollback and then the screen. Each
    /// row is trimmed on the right, and blank rows at the end are left out.
    pub(super) fn capture(&self, id: PaneId, history: usize) -> Result<String, String> {
        let p = self.panes.get(&id).ok_or_else(|| no_pane(id))?;
        let term = p.term.lock();
        let grid = term.grid();
        let hist = grid.history_size().min(history) as i32;
        let (rows, cols) = (term.screen_lines() as i32, term.columns());
        let mut lines: Vec<String> = (-hist..rows)
            .map(|l| {
                let row = &grid[Line(l)];
                let mut s = String::with_capacity(cols);
                for c in 0..cols {
                    let cell = &row[Column(c)];
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }
                    s.push(if cell.c.is_control() { ' ' } else { cell.c });
                    if let Some(extra) = cell.zerowidth() {
                        s.extend(extra);
                    }
                }
                s.truncate(s.trim_end().len());
                s
            })
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let mut out = lines.join("\n");
        out.push('\n');
        Ok(out)
    }
}

fn no_pane(id: PaneId) -> String {
    format!("no pane {id} (see `ranma panes`)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        App::new(config, tx, 80, 24)
    }

    fn ask(a: &mut App, q: Query) -> Result<String, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        a.answer(q, tx);
        rx.try_recv().expect("answered at once")
    }

    #[test]
    fn wait_on_a_pane_that_already_ended_answers_its_status() {
        let mut a = app();
        a.next_id = 5;
        a.exit_codes.insert(3, 7);
        a.pane_ended(3);
        assert_eq!(ask(&mut a, Query::Wait { pane: 3 }), Ok("7\n".into()));
        // Ended, status unknown (closed by ranma, or long forgotten).
        assert_eq!(ask(&mut a, Query::Wait { pane: 2 }), Ok(String::new()));
        // Never existed.
        assert!(ask(&mut a, Query::Wait { pane: 9 }).is_err());
        assert!(ask(&mut a, Query::Wait { pane: 0 }).is_err());
    }

    #[test]
    fn waiters_hear_when_their_pane_ends() {
        let mut a = app();
        let (tx, rx) = std::sync::mpsc::channel();
        a.waiters.entry(4).or_default().push(tx);
        a.exit_codes.insert(4, 0);
        a.pane_ended(4);
        assert_eq!(rx.try_recv(), Ok(Ok("0\n".into())));
    }
}
