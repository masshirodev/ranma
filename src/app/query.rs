//! Answering queries from the socket: `ranma open`, `panes`, `send`,
//! `capture` and `wait` (see `ipc`). Each gets a reply on its own channel; the
//! connection's thread is waiting on it.

use std::sync::mpsc::Sender;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;

use super::{App, SCRATCHPAD, chord_bytes};
use crate::input;
use crate::ipc::{PaneInfo, Query, SendInput};
use crate::layout::PaneId;
use crate::workspace::Workspace;

pub type Reply = Sender<Result<String, String>>;

impl App {
    pub(super) fn answer(&mut self, q: Query, reply: Reply) {
        let result = match q {
            Query::Open(spec) => self.open_spec(spec).map(|id| format!("{id}\n")),
            Query::Panes => serde_json::to_string(&self.pane_infos())
                .map(|j| j + "\n")
                .map_err(|e| e.to_string()),
            Query::Send { pane, input } => self.send_input(pane, &input).map(|_| String::new()),
            Query::Capture { pane, history } => self.capture(pane, history),
            Query::Wait { pane } => {
                if self.panes.contains_key(&pane) {
                    self.waiters.entry(pane).or_default().push(reply);
                    return;
                }
                Err(no_pane(pane))
            }
        };
        let _ = reply.send(result);
    }

    /// A pane is gone: whoever waits on it gets its exit status.
    pub(super) fn pane_ended(&mut self, id: PaneId) {
        let code = self.exit_codes.remove(&id);
        for w in self.waiters.remove(&id).unwrap_or_default() {
            let _ = w.send(Ok(code.map(|c| format!("{c}\n")).unwrap_or_default()));
        }
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
                })
            })
            .collect()
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
