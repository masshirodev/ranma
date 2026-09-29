//! The few OSC sequences ranma wants that alacritty_terminal drops: shell
//! marks (OSC 133) and desktop notifications (OSC 9, OSC 777).
//!
//! This sits on the PTY path, before the bytes reach the emulator, so it must
//! cost next to nothing: a read with no ESC in it is one search for the byte
//! and done. Only after `ESC ]` does it look at bytes one by one, and only
//! until the sequence ends or runs past `MAX`, which ends the looking. A
//! sequence split across reads is followed through the split. It never
//! changes the bytes; the emulator still sees everything.

use std::time::{Duration, Instant};

/// Longer than any mark, notification or nested ranma's report worth keeping.
const MAX: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A command ran to its end (133;D after 133;C): its status if the shell
    /// said, and how long it ran.
    CommandFinished {
        exit: Option<i32>,
        duration: Duration,
    },
    /// A program asked for a desktop notification (OSC 9, or 777;notify).
    Notify { title: String, body: String },
    /// A ranma starting in the pane asks whether a ranma draws around it
    /// (`nestbar::HELLO`).
    RanmaHello,
    /// A ranma in the pane reports its workspaces (JSON; see `nestbar`).
    RanmaReport(String),
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Ground,
    /// Just read ESC.
    Esc,
    /// Inside `ESC ]`, collecting the payload.
    Osc,
    /// Inside an OSC, just read ESC: `\` ends it.
    OscEsc,
    /// An OSC too long to be one of ours: skipped to its end.
    Skip,
    SkipEsc,
}

#[derive(Debug, Default)]
pub struct Scanner {
    state: State,
    payload: Vec<u8>,
    /// When the running command started (133;C), if one is running.
    started: Option<Instant>,
}

impl Scanner {
    /// Feed bytes as read from the PTY; returns what they finished saying.
    pub fn feed(&mut self, bytes: &[u8], now: Instant) -> Vec<Mark> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if matches!(self.state, State::Ground) {
                // The hot path: nothing to do until an ESC.
                match bytes[i..].iter().position(|b| *b == 0x1b) {
                    Some(p) => {
                        i += p + 1;
                        self.state = State::Esc;
                    }
                    None => return out,
                }
                continue;
            }
            let b = bytes[i];
            i += 1;
            self.state = match std::mem::take(&mut self.state) {
                State::Ground => unreachable!("handled above"),
                State::Esc if b == b']' => {
                    self.payload.clear();
                    State::Osc
                }
                State::Esc if b == 0x1b => State::Esc,
                State::Esc => State::Ground,
                State::Osc | State::OscEsc if b == 0x07 => {
                    self.finish(now, &mut out);
                    State::Ground
                }
                State::OscEsc if b == b'\\' => {
                    self.finish(now, &mut out);
                    State::Ground
                }
                // ESC not followed by `\` inside an OSC: the OSC is cut off
                // and a new sequence starts, as a terminal would take it.
                State::OscEsc if b == b']' => {
                    self.payload.clear();
                    State::Osc
                }
                State::OscEsc => State::Ground,
                State::Osc if b == 0x1b => State::OscEsc,
                State::Osc if self.payload.len() >= MAX => State::Skip,
                State::Osc => {
                    self.payload.push(b);
                    State::Osc
                }
                State::Skip | State::SkipEsc if b == 0x07 => State::Ground,
                State::SkipEsc if b == b'\\' => State::Ground,
                State::Skip if b == 0x1b => State::SkipEsc,
                State::Skip | State::SkipEsc => State::Skip,
            };
        }
        out
    }

    fn finish(&mut self, now: Instant, out: &mut Vec<Mark>) {
        let payload = String::from_utf8_lossy(&self.payload).into_owned();
        self.payload.clear();
        let mut parts = payload.splitn(2, ';');
        let (code, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        match code {
            "133" => {
                let mut f = rest.split(';');
                match f.next() {
                    Some("C") => self.started = Some(now),
                    // D without a C before it is a prompt after nothing ran
                    // (the shell's first prompt, or an empty Enter).
                    Some("D") => {
                        if let Some(start) = self.started.take() {
                            out.push(Mark::CommandFinished {
                                exit: f.next().and_then(|s| s.trim().parse().ok()),
                                duration: now.saturating_duration_since(start),
                            });
                        }
                    }
                    _ => {}
                }
            }
            // OSC 9 is also ConEmu's family of `9;N;...` commands (progress
            // bars, mostly): a digit and a semicolon is one of those, not text.
            "9" if !rest.is_empty() && !is_conemu(rest) => out.push(Mark::Notify {
                title: String::new(),
                body: rest.to_string(),
            }),
            crate::nestbar::OSC => match rest {
                "?" => out.push(Mark::RanmaHello),
                r => {
                    if let Some(json) = r.strip_prefix("report;") {
                        out.push(Mark::RanmaReport(json.to_string()));
                    }
                }
            },
            "777" => {
                let mut f = rest.splitn(3, ';');
                if f.next() == Some("notify") {
                    out.push(Mark::Notify {
                        title: f.next().unwrap_or("").to_string(),
                        body: f.next().unwrap_or("").to_string(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn is_conemu(rest: &str) -> bool {
    let mut c = rest.chars();
    c.next().is_some_and(|d| d.is_ascii_digit()) && matches!(c.next(), Some(';') | None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_timed_from_c_to_d() {
        let t0 = Instant::now();
        let mut s = Scanner::default();
        assert!(
            s.feed(b"\x1b]133;D;0\x07\x1b]133;A\x07$ ", t0).is_empty(),
            "D with no C"
        );
        assert!(s.feed(b"make\r\n\x1b]133;C\x07building...", t0).is_empty());
        let later = t0 + Duration::from_secs(42);
        assert_eq!(
            s.feed(b"done\r\n\x1b]133;D;2\x1b\\\x1b]133;A\x07$ ", later),
            vec![Mark::CommandFinished {
                exit: Some(2),
                duration: Duration::from_secs(42)
            }]
        );
        // Once only: the next D has no C before it.
        assert!(s.feed(b"\x1b]133;D;0\x07", later).is_empty());
    }

    #[test]
    fn sequences_split_across_reads_are_followed() {
        let t = Instant::now();
        let mut s = Scanner::default();
        let whole = b"x\x1b]133;C\x07out\x1b]133;D;7\x1b\\";
        let mut marks = Vec::new();
        for chunk in whole.chunks(1) {
            marks.extend(s.feed(chunk, t));
        }
        assert_eq!(
            marks,
            vec![Mark::CommandFinished {
                exit: Some(7),
                duration: Duration::ZERO
            }]
        );
    }

    #[test]
    fn notifications_from_osc_9_and_777() {
        let t = Instant::now();
        let mut s = Scanner::default();
        assert_eq!(
            s.feed(
                b"\x1b]9;build done\x07\x1b]777;notify;make;all green\x1b\\",
                t
            ),
            vec![
                Mark::Notify {
                    title: String::new(),
                    body: "build done".into()
                },
                Mark::Notify {
                    title: "make".into(),
                    body: "all green".into()
                },
            ]
        );
        // ConEmu's progress bar is not a notification.
        assert!(s.feed(b"\x1b]9;4;1;50\x07", t).is_empty());
    }

    #[test]
    fn a_nested_ranma_asks_and_reports() {
        let t = Instant::now();
        let mut s = Scanner::default();
        let mut bytes = crate::nestbar::HELLO.as_bytes().to_vec();
        bytes.extend(b"\x1b]51377;report;{\"v\":1}\x07\x1b]51377;ranma;1\x07");
        assert_eq!(
            s.feed(&bytes, t),
            vec![Mark::RanmaHello, Mark::RanmaReport("{\"v\":1}".into())]
        );
    }

    #[test]
    fn other_sequences_and_floods_pass_untouched() {
        let t = Instant::now();
        let mut s = Scanner::default();
        // A title, colours, a CSI, and an OSC far too long: nothing, no state kept.
        assert!(s.feed(b"\x1b]0;title\x07\x1b[31mred\x1b[0m", t).is_empty());
        let mut long = b"\x1b]9;".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX * 2));
        long.extend(b"\x07\x1b]9;after\x07");
        assert_eq!(
            s.feed(&long, t),
            vec![Mark::Notify {
                title: String::new(),
                body: "after".into()
            }]
        );
        assert!(s.payload.capacity() <= MAX * 2);
    }
}
