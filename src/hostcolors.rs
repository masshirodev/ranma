//! The host terminal's colours, so ranma can answer programs that ask.
//!
//! Programs query the terminal for its foreground, background and palette (OSC 10,
//! 11 and 4): nvim picks light or dark from the background, many tools do the
//! same. Inside ranma the "terminal" is ranma, which has no colours of its own —
//! it draws with the host's. So ranma asks the host once at startup and answers
//! from that. Without it the queries go unanswered and every such program waits
//! for its timeout and then guesses.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostColors {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub cursor: Option<Rgb>,
    /// The 16 ANSI colours as the host has them.
    pub palette: [Option<Rgb>; 16],
}

impl HostColors {
    /// The colour at alacritty_terminal's index: 0-255 the palette, 256 the
    /// foreground, 257 the background, 258 the cursor. Palette entries past 15
    /// are the standard xterm cube and grey ramp, which no terminal changes.
    pub fn get(&self, index: usize) -> Option<Rgb> {
        match index {
            0..=15 => self.palette[index],
            16..=255 => Some(xterm_256(index as u8)),
            256 => self.fg,
            257 => self.bg,
            258 => self.cursor.or(self.fg),
            _ => None,
        }
    }
}

/// The 6x6x6 cube and 24-step grey ramp of the xterm 256-colour palette.
pub fn xterm_256(i: u8) -> Rgb {
    if i >= 232 {
        let v = 8 + 10 * (i - 232);
        return Rgb { r: v, g: v, b: v };
    }
    let i = i.saturating_sub(16);
    let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
    Rgb {
        r: level(i / 36),
        g: level((i / 6) % 6),
        b: level(i % 6),
    }
}

/// Parse `rgb:RRRR/GGGG/BBBB` (1-4 hex digits per channel, as terminals send it).
pub fn parse_rgb(spec: &str) -> Option<Rgb> {
    let body = spec.strip_prefix("rgb:")?;
    let mut parts = body.split('/');
    let mut chan = || -> Option<u8> {
        let h = parts.next()?;
        if h.is_empty() || h.len() > 4 {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        let max = (1u32 << (4 * h.len())) - 1;
        Some(((v * 255 + max / 2) / max) as u8)
    };
    let c = Rgb {
        r: chan()?,
        g: chan()?,
        b: chan()?,
    };
    parts.next().is_none().then_some(c)
}

/// What the host sent while being asked, minus its replies (OSC answers and the
/// DA1 reply): keys typed while ranma was starting, which belong to the first
/// pane rather than to the bin.
pub fn leftover_input(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i..].starts_with(b"\x1b]") || input[i..].starts_with(b"\x1b_") {
            // An OSC reply, or the graphics query's (an APC): up to BEL or ST.
            let rest = &input[i + 2..];
            let end = rest
                .iter()
                .enumerate()
                .find_map(|(j, b)| match b {
                    0x07 => Some(j + 1),
                    0x1b if rest.get(j + 1) == Some(&b'\\') => Some(j + 2),
                    _ => None,
                })
                .unwrap_or(rest.len());
            i += 2 + end;
        } else if input[i..].starts_with(b"\x1b[?") {
            // The DA1 reply (ESC [ ? digits and ; then c), or the keyboard
            // protocol's (ESC [ ? digits u).
            let rest = &input[i + 3..];
            let n = rest
                .iter()
                .take_while(|b| b.is_ascii_digit() || **b == b';')
                .count();
            if matches!(rest.get(n), Some(b'c' | b'u')) {
                i += 3 + n + 1;
            } else {
                out.push(input[i]);
                i += 1;
            }
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

/// Pull every OSC 4/10/11/12 colour reply out of what the host sent.
pub fn parse_replies(input: &[u8]) -> HostColors {
    let mut out = HostColors::default();
    let text = String::from_utf8_lossy(input);
    for chunk in text.split("\x1b]").skip(1) {
        // Replies end in BEL or ST (ESC \); cut at whichever comes first.
        let end = chunk.find(['\x07', '\x1b']).unwrap_or(chunk.len());
        let body = &chunk[..end];
        let mut fields = body.split(';');
        match fields.next() {
            Some("10") => out.fg = fields.next().and_then(parse_rgb),
            Some("11") => out.bg = fields.next().and_then(parse_rgb),
            Some("12") => out.cursor = fields.next().and_then(parse_rgb),
            Some("4") => {
                if let (Some(Ok(i)), Some(spec)) =
                    (fields.next().map(str::parse::<usize>), fields.next())
                    && i < 16
                {
                    out.palette[i] = parse_rgb(spec);
                }
            }
            _ => {}
        }
    }
    out
}

/// Ask the host for its colours. Must run in raw mode, before anything else reads
/// the terminal's input.
///
/// The queries are followed by DA1 (`ESC [ c`), which every terminal answers,
/// after its answers to the queries before it. Reading stops at that reply, so a
/// terminal that ignores colour queries costs one round trip, not a timeout, and
/// no late reply is left in the input to be read as keystrokes later.
///
/// Returns the colours, and any other input that arrived meanwhile (see
/// `leftover_input`).
pub fn query(timeout: Duration) -> (HostColors, Vec<u8>) {
    let r = query_all(timeout);
    (r.colors, r.typed_early)
}

/// `query`, also asking whether a ranma draws around this one (see
/// `nestbar`): its protocol and colours, if one answered. The question goes before DA1,
/// so an outer ranma's answer comes before the DA1 reply that ends reading.
pub fn query_all(timeout: Duration) -> Replies {
    let mut q = String::from("\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b]12;?\x1b\\");
    for i in 0..16 {
        q.push_str(&format!("\x1b]4;{i};?\x1b\\"));
    }
    q.push_str(crate::nestbar::HELLO);
    q.push_str(GRAPHICS_QUERY);
    // The kitty keyboard protocol: answered `CSI ? flags u` by a terminal
    // that speaks it.
    q.push_str("\x1b[?u");
    q.push_str("\x1b[c");
    let mut stdout = std::io::stdout();
    if stdout
        .write_all(q.as_bytes())
        .and_then(|_| stdout.flush())
        .is_err()
    {
        return Replies::default();
    }

    let fd = std::io::stdin().as_raw_fd();
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd for the duration of the call.
        let ready = unsafe { libc::poll(&mut pfd, 1, left.as_millis() as libc::c_int) };
        if ready <= 0 {
            break;
        }
        // SAFETY: reading into a stack buffer of the length given.
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        if da1_seen(&buf) {
            break;
        }
    }
    Replies {
        colors: parse_replies(&buf),
        typed_early: leftover_input(&buf),
        outer: crate::nestbar::outer_in(&buf),
        outer_colors: crate::nestbar::outer_colors_in(&buf),
        graphics: graphics_in(&buf),
        kitty_keys: kitty_keys_in(&buf),
    }
}

/// Whether the terminal shows kitty graphics: a one-pixel image it is asked
/// to check and not keep (`a=q`), answered OK by kitty, ghostty and an outer
/// ranma whose own terminal does. One that does not know it says nothing.
pub const GRAPHICS_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

/// Whether the replies hold the graphics query's OK.
pub fn graphics_in(input: &[u8]) -> bool {
    let text = String::from_utf8_lossy(input);
    text.split("\x1b_G").skip(1).any(|r| {
        let body = &r[..r.find('\x1b').unwrap_or(r.len())];
        body.starts_with("i=31") && body.ends_with(";OK")
    })
}

/// What the terminal answered at start (`query_all`).
#[derive(Debug, Default)]
pub struct Replies {
    pub colors: HostColors,
    /// Keys typed meanwhile; they belong to the focused pane.
    pub typed_early: Vec<u8>,
    /// The protocol of a ranma around this one, if one answered.
    pub outer: Option<u32>,
    /// Its theme's colours, if it sent them (see `nestbar::colors_osc`).
    pub outer_colors: Option<serde_json::Map<String, serde_json::Value>>,
    /// It answered the graphics query: it can show images.
    pub graphics: bool,
    /// It answered `CSI ? u`: it speaks the kitty keyboard protocol.
    pub kitty_keys: bool,
}

/// Whether the replies hold an answer to `CSI ? u` (`CSI ? <digits> u`).
pub fn kitty_keys_in(buf: &[u8]) -> bool {
    (0..buf.len()).any(|i| {
        buf[i..].starts_with(b"\x1b[?")
            && buf[i + 3..].iter().find(|b| !b.is_ascii_digit()) == Some(&b'u')
    })
}

/// Whether a DA1 reply (`ESC [ ? <digits and ;> c`) has arrived.
fn da1_seen(buf: &[u8]) -> bool {
    (0..buf.len()).any(|i| {
        buf[i..].starts_with(b"\x1b[?")
            && buf[i + 3..]
                .iter()
                .find(|b| !(b.is_ascii_digit() || **b == b';'))
                == Some(&b'c')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_widths_scale_to_eight_bits() {
        assert_eq!(
            parse_rgb("rgb:ffff/0000/8080"),
            Some(Rgb {
                r: 255,
                g: 0,
                b: 128
            })
        );
        assert_eq!(
            parse_rgb("rgb:ff/00/80"),
            Some(Rgb {
                r: 255,
                g: 0,
                b: 128
            })
        );
        assert_eq!(
            parse_rgb("rgb:f/0/8"),
            Some(Rgb {
                r: 255,
                g: 0,
                b: 136
            })
        );
        assert_eq!(
            parse_rgb("rgb:1e1e/1e1e/2e2e"),
            Some(Rgb {
                r: 30,
                g: 30,
                b: 46
            })
        );
        assert_eq!(parse_rgb("rgb:ff/00"), None);
        assert_eq!(parse_rgb("#ff0000"), None);
    }

    #[test]
    fn replies_are_picked_out_of_a_stream() {
        let input = b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x07\
                      \x1b]4;1;rgb:f3/8b/a8\x1b\\\x1b]4;99;rgb:00/00/00\x07\x1b[?62;22c";
        let c = parse_replies(input);
        assert_eq!(
            c.fg,
            Some(Rgb {
                r: 205,
                g: 214,
                b: 244
            })
        );
        assert_eq!(
            c.bg,
            Some(Rgb {
                r: 30,
                g: 30,
                b: 46
            })
        );
        assert_eq!(
            c.palette[1],
            Some(Rgb {
                r: 243,
                g: 139,
                b: 168
            })
        );
        assert_eq!(c.cursor, None);
        assert!(da1_seen(input));
        assert!(!da1_seen(b"\x1b]10;rgb:0/0/0\x07"));
        assert!(!da1_seen(b"\x1b[?62;22"));
    }

    #[test]
    fn keys_typed_during_the_query_are_kept() {
        let input = b"ls\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b]10;rgb:0/0/0\x07 -la\x1b[?62;22c\r";
        assert_eq!(leftover_input(input), b"ls -la\r");
        assert_eq!(
            leftover_input(b"\x1b[A"),
            b"\x1b[A",
            "an arrow key is not a reply"
        );
        assert!(leftover_input(b"\x1b[?6c").is_empty());
    }

    #[test]
    fn index_mapping() {
        let c = HostColors {
            fg: Some(Rgb { r: 1, g: 2, b: 3 }),
            bg: Some(Rgb { r: 4, g: 5, b: 6 }),
            ..Default::default()
        };
        assert_eq!(c.get(256), c.fg);
        assert_eq!(c.get(257), c.bg);
        assert_eq!(c.get(258), c.fg, "cursor falls back to the foreground");
        assert_eq!(c.get(3), None, "an unanswered palette entry stays unknown");
        assert_eq!(c.get(16), Some(Rgb { r: 0, g: 0, b: 0 }));
        assert_eq!(
            c.get(231),
            Some(Rgb {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(c.get(232), Some(Rgb { r: 8, g: 8, b: 8 }));
        assert_eq!(c.get(196), Some(Rgb { r: 255, g: 0, b: 0 }));
    }

    #[test]
    fn the_graphics_answer_is_read_and_not_typed() {
        let input = b"\x1b]11;rgb:00/00/00\x1b\\\x1b_Gi=31;OK\x1b\\ls\x1b[?62;c";
        assert!(graphics_in(input));
        assert_eq!(leftover_input(input), b"ls");
        assert!(!graphics_in(b"\x1b_Gi=31;ENOTSUPPORTED\x1b\\\x1b[?62;c"));
        assert!(!graphics_in(b"\x1b[?62;c"));
    }

    #[test]
    fn the_keyboard_protocols_answer_is_read_and_not_typed() {
        let input = b"\x1b[?0uq\x1b[?62;22c";
        assert!(kitty_keys_in(input));
        assert_eq!(leftover_input(input), b"q");
        assert!(!kitty_keys_in(b"\x1b[?62;22c"));
    }
}
