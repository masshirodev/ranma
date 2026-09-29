//! What a ranma client and its server say to each other.
//!
//! The server owns everything; the client owns nothing (see DESIGN.md, "A daemon,
//! and a client that holds nothing"). So the protocol is small: the client sends
//! what the terminal gives it (keys, mouse, resizes) and a hello; the server sends
//! bytes for the terminal, and says when the client should go.
//!
//! On the wire every message is a frame: one kind byte, a u32 length (big
//! endian), then the payload. Terminal output travels raw; everything else is
//! JSON, which crossterm's events already serialise to.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

use crate::hostcolors::HostColors;

/// A frame bigger than this is not one ranma sent: refuse it rather than
/// allocating whatever a corrupt length says.
const MAX_FRAME: u32 = 64 * 1024 * 1024;

/// The first thing a client sends after the `attach` line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    /// The client's build (update::BUILD_SHA): a server from another build says so.
    pub build: String,
    pub cols: u16,
    pub rows: u16,
    /// The colours of the terminal the client runs in, asked at its start.
    pub colors: HostColors,
    /// Keys typed while the client was starting; they belong to the focused pane.
    pub typed_early: Vec<u8>,
    /// The socket of the ranma the client itself runs inside (its `RANMA_SOCKET`),
    /// if any: switching this client there would feed that server into itself.
    /// Defaulted, so a server and a client a build apart still understand a hello.
    #[serde(default)]
    pub inside: Option<String>,
    /// The client was reached over SSH: the title names this host (see the
    /// `title_host` setting). Defaulted like `inside`.
    #[serde(default)]
    pub remote: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToServer {
    Hello(Hello),
    Event(crossterm::event::Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToClient {
    /// Bytes for the terminal, as they are.
    Output(Vec<u8>),
    /// The client should leave; the server keeps running.
    Detached(String),
    /// The server is gone (quit, or its last pane closed).
    Exited(String),
    /// Attach to the server with this name instead; this one lets the client go.
    Switch(String),
}

/// What `status` answers: enough for a client to pick a server to attach to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Status {
    pub name: String,
    pub attached: bool,
    pub panes: usize,
    pub sessions: Vec<String>,
    /// Seconds since the Unix epoch of the last input from a client.
    pub last_active: u64,
    pub build: String,
}

const K_HELLO: u8 = 1;
const K_EVENT: u8 = 2;
const K_OUTPUT: u8 = 10;
const K_DETACHED: u8 = 11;
const K_EXITED: u8 = 12;
const K_SWITCH: u8 = 13;

fn write_frame(w: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too big"))?;
    let mut head = [0u8; 5];
    head[0] = kind;
    head[1..].copy_from_slice(&len.to_be_bytes());
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// One frame, or `None` at a clean end of stream.
fn read_frame(r: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match r.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(head[1..].try_into().expect("four bytes"));
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame of {len} bytes"),
        ));
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    Ok(Some((head[0], payload)))
}

fn bad(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

pub fn send_to_server(w: &mut impl Write, m: &ToServer) -> io::Result<()> {
    match m {
        ToServer::Hello(h) => write_frame(w, K_HELLO, &serde_json::to_vec(h).map_err(bad)?),
        ToServer::Event(e) => write_frame(w, K_EVENT, &serde_json::to_vec(e).map_err(bad)?),
    }
}

pub fn read_to_server(r: &mut impl Read) -> io::Result<Option<ToServer>> {
    let Some((kind, p)) = read_frame(r)? else {
        return Ok(None);
    };
    Ok(Some(match kind {
        K_HELLO => ToServer::Hello(serde_json::from_slice(&p).map_err(bad)?),
        K_EVENT => ToServer::Event(serde_json::from_slice(&p).map_err(bad)?),
        k => return Err(bad(format!("unknown frame kind {k} from a client"))),
    }))
}

pub fn send_to_client(w: &mut impl Write, m: &ToClient) -> io::Result<()> {
    match m {
        ToClient::Output(b) => write_frame(w, K_OUTPUT, b),
        ToClient::Detached(s) => write_frame(w, K_DETACHED, s.as_bytes()),
        ToClient::Exited(s) => write_frame(w, K_EXITED, s.as_bytes()),
        ToClient::Switch(s) => write_frame(w, K_SWITCH, s.as_bytes()),
    }
}

pub fn read_to_client(r: &mut impl Read) -> io::Result<Option<ToClient>> {
    let Some((kind, p)) = read_frame(r)? else {
        return Ok(None);
    };
    Ok(Some(match kind {
        K_OUTPUT => ToClient::Output(p),
        K_DETACHED => ToClient::Detached(String::from_utf8_lossy(&p).into_owned()),
        K_EXITED => ToClient::Exited(String::from_utf8_lossy(&p).into_owned()),
        K_SWITCH => ToClient::Switch(String::from_utf8_lossy(&p).into_owned()),
        k => return Err(bad(format!("unknown frame kind {k} from the server"))),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn messages_round_trip() {
        let hello = Hello {
            build: "abc".into(),
            cols: 120,
            rows: 40,
            colors: HostColors::default(),
            typed_early: b"ls\r".to_vec(),
            inside: Some("/run/user/1000/ranma/1.sock".into()),
            remote: true,
        };
        let mut buf = Vec::new();
        send_to_server(&mut buf, &ToServer::Hello(hello.clone())).unwrap();
        let key = Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        send_to_server(&mut buf, &ToServer::Event(key.clone())).unwrap();
        send_to_server(&mut buf, &ToServer::Event(Event::Resize(80, 24))).unwrap();
        let mut r = &buf[..];
        assert_eq!(
            read_to_server(&mut r).unwrap(),
            Some(ToServer::Hello(hello))
        );
        assert_eq!(read_to_server(&mut r).unwrap(), Some(ToServer::Event(key)));
        assert_eq!(
            read_to_server(&mut r).unwrap(),
            Some(ToServer::Event(Event::Resize(80, 24)))
        );
        assert_eq!(read_to_server(&mut r).unwrap(), None, "clean end of stream");

        let mut buf = Vec::new();
        let out = ToClient::Output(b"\x1b[2J hello \xff".to_vec());
        send_to_client(&mut buf, &out).unwrap();
        send_to_client(&mut buf, &ToClient::Detached("bye".into())).unwrap();
        send_to_client(&mut buf, &ToClient::Exited("gone".into())).unwrap();
        send_to_client(&mut buf, &ToClient::Switch("2".into())).unwrap();
        let mut r = &buf[..];
        assert_eq!(read_to_client(&mut r).unwrap(), Some(out));
        assert_eq!(
            read_to_client(&mut r).unwrap(),
            Some(ToClient::Detached("bye".into()))
        );
        assert_eq!(
            read_to_client(&mut r).unwrap(),
            Some(ToClient::Exited("gone".into()))
        );
        assert_eq!(
            read_to_client(&mut r).unwrap(),
            Some(ToClient::Switch("2".into()))
        );
        assert_eq!(read_to_client(&mut r).unwrap(), None);
    }

    #[test]
    fn a_hello_from_an_older_build_still_reads() {
        // Before `inside` existed: a server must still take this client.
        let mut old = serde_json::to_value(Hello {
            build: "x".into(),
            cols: 80,
            rows: 24,
            colors: HostColors::default(),
            typed_early: Vec::new(),
            inside: None,
            remote: false,
        })
        .unwrap();
        old.as_object_mut().unwrap().remove("inside");
        old.as_object_mut().unwrap().remove("remote");
        let h: Hello = serde_json::from_value(old).unwrap();
        assert_eq!(h.inside, None);
    }

    #[test]
    fn corrupt_frames_are_refused() {
        // A length far beyond anything ranma sends.
        let mut r: &[u8] = &[K_OUTPUT, 0xff, 0xff, 0xff, 0xff];
        assert!(read_to_client(&mut r).is_err());
        // Unknown kinds, both ways.
        let mut r: &[u8] = &[99, 0, 0, 0, 0];
        assert!(read_to_client(&mut r).is_err());
        let mut r: &[u8] = &[99, 0, 0, 0, 0];
        assert!(read_to_server(&mut r).is_err());
        // A stream cut mid-frame is an error, not a clean end.
        let mut r: &[u8] = &[K_OUTPUT, 0, 0, 0, 9, b'a'];
        assert!(read_to_client(&mut r).is_err());
    }
}
