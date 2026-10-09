//! Images in panes: the kitty graphics protocol, by Unicode placeholders
//! (DESIGN.md, "Images in panes").
//!
//! A program sends an image in an APC sequence (`ESC _ G keys ; data ESC \`)
//! and shows it by printing placeholder cells: U+10EEEE with the image id in
//! the foreground colour and the row and column in combining marks. Those
//! cells are text, so they live in the pane's grid, scroll, clip and come
//! back from scrollback like any other. ranma's part is the transmission:
//! the scanner on the PTY path hands each APC here, and this module
//!
//! - gives every (pane, image id) an id of its own on the host terminal, so
//!   two panes cannot overwrite each other's images, and the renderer
//!   rewrites the placeholder cells' colour to that id ([`Graphics::host_id`]);
//! - reads files, temporary files and shared memory the program named, on
//!   the machine the program runs on, and sends the bytes instead, so it
//!   works when the terminal is across an ssh connection;
//! - answers the program itself (queries, OK), and silences the host's
//!   answers, which would arrive as typed keys;
//! - keeps what it sent, to send again to a terminal that attaches later.
//!
//! Direct placements (no `U=1`) are not shown: the host would draw them at
//! its own cursor, which is not where the pane's is.

use std::collections::{BTreeMap, HashMap};

use crate::layout::PaneId;

/// The placeholder character.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// The combining marks that number rows and columns (and the image id's
/// high byte) after a placeholder, from kitty's rowcolumn-diacritics.txt.
pub const DIACRITICS: [u32; 297] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186,
    0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

/// What is kept to send again, at most; the oldest images go first.
const STORE_CAP: usize = 32 << 20;
/// A file the program names is read up to this size.
const FILE_CAP: u64 = 64 << 20;
/// Base64 bytes per chunk, as the protocol asks.
const CHUNK: usize = 4096;

/// One command: its keys in order, and its payload as sent (base64 or a
/// base64 path).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub keys: Vec<(String, String)>,
    pub payload: Vec<u8>,
}

impl Command {
    /// An APC body (`G` and what follows, without `ESC _` and `ESC \`).
    pub fn parse(body: &[u8]) -> Option<Command> {
        let rest = body.strip_prefix(b"G")?;
        let (control, payload) = match rest.iter().position(|b| *b == b';') {
            Some(p) => (&rest[..p], rest[p + 1..].to_vec()),
            None => (rest, Vec::new()),
        };
        let control = std::str::from_utf8(control).ok()?;
        let keys = control
            .split(',')
            .filter(|kv| !kv.is_empty())
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect();
        Some(Command { keys, payload })
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    }

    fn num(&self, k: &str) -> Option<u32> {
        self.get(k).and_then(|v| v.parse().ok())
    }

    fn set(&mut self, k: &str, v: impl ToString) {
        match self.keys.iter_mut().find(|(key, _)| key == k) {
            Some(kv) => kv.1 = v.to_string(),
            None => self.keys.push((k.to_string(), v.to_string())),
        }
    }

    fn remove(&mut self, k: &str) {
        self.keys.retain(|(key, _)| key != k);
    }

    /// The whole sequence, `ESC _ G ... ESC \`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = b"\x1b_G".to_vec();
        let control: Vec<String> = self.keys.iter().map(|(k, v)| format!("{k}={v}")).collect();
        out.extend_from_slice(control.join(",").as_bytes());
        if !self.payload.is_empty() {
            out.push(b';');
            out.extend_from_slice(&self.payload);
        }
        out.extend_from_slice(b"\x1b\\");
        out
    }

    fn action(&self) -> char {
        self.get("a").and_then(|a| a.chars().next()).unwrap_or('t')
    }

    fn quiet(&self) -> u32 {
        self.num("q").unwrap_or(0)
    }
}

/// What handling a command produced: bytes for the host terminals, and an
/// answer to write back to the pane.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Out {
    pub host: Vec<Vec<u8>>,
    pub reply: Vec<u8>,
}

/// A chunked transmission under way in a pane.
#[derive(Debug)]
struct Inflight {
    host: u32,
    /// The answer to give when it is complete, if one is due.
    reply: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
struct Image {
    pane: PaneId,
    prog: u32,
    /// The transmission as sent to the host, in order: what a terminal
    /// attaching later is sent.
    data: Vec<Vec<u8>>,
    /// Virtual placements, by placement id.
    placements: BTreeMap<u32, Vec<u8>>,
}

impl Image {
    fn bytes(&self) -> usize {
        self.data.iter().map(Vec::len).sum::<usize>()
            + self.placements.values().map(Vec::len).sum::<usize>()
    }
}

#[derive(Debug, Default)]
pub struct Graphics {
    /// The last host id given out.
    last: u32,
    /// (pane, the program's id) to the host's.
    ids: HashMap<(PaneId, u32), u32>,
    /// (pane, image number) to the program's id: `I=` is answered with an id.
    numbers: HashMap<(PaneId, u32), u32>,
    inflight: HashMap<PaneId, Inflight>,
    images: BTreeMap<u32, Image>,
}

impl Graphics {
    /// The host's id for the image a pane's program calls `prog`.
    pub fn host_id(&self, pane: PaneId, prog: u32) -> Option<u32> {
        self.ids.get(&(pane, prog)).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// One APC from a pane. `supported`: a terminal attached can show
    /// images, which is what a query is answered by.
    pub fn handle(&mut self, pane: PaneId, body: &[u8], supported: bool) -> Out {
        let mut out = Out::default();
        let Some(cmd) = Command::parse(body) else {
            return out;
        };
        // The rest of a chunked transmission carries no action and no id.
        if self.inflight.contains_key(&pane)
            && cmd.get("a").is_none()
            && cmd.get("i").is_none()
            && cmd.get("I").is_none()
        {
            self.continue_chunk(pane, cmd, &mut out);
            return out;
        }
        self.inflight.remove(&pane);
        match cmd.action() {
            // q=1 silences the OK, q=2 errors too.
            'q' => match (supported, cmd.quiet()) {
                (true, 0) => out.reply = answer(&cmd, None, "OK"),
                (false, 0 | 1) => {
                    out.reply = answer(
                        &cmd,
                        None,
                        "ENOTSUPPORTED:the terminal ranma runs in cannot show images",
                    )
                }
                _ => {}
            },
            't' | 'T' => self.transmit(pane, cmd, &mut out),
            'p' => self.place(pane, cmd, &mut out),
            'd' => self.delete(pane, cmd, &mut out),
            // Animation: frames, their control and composition, for an
            // image already sent.
            'f' | 'a' | 'c' => {
                if let Some(host) = self.prog_of(pane, &cmd).and_then(|p| self.host_id(pane, p)) {
                    let mut c = cmd;
                    rewrite(&mut c, host);
                    let bytes = c.encode();
                    if let Some(img) = self.images.get_mut(&host) {
                        img.data.push(bytes.clone());
                    }
                    out.host.push(bytes);
                }
            }
            _ => {}
        }
        self.evict();
        out
    }

    /// Everything kept, for a terminal that just attached.
    pub fn replay(&self) -> Vec<Vec<u8>> {
        self.images
            .values()
            .flat_map(|img| img.data.iter().chain(img.placements.values()))
            .cloned()
            .collect()
    }

    /// A pane closed: its images are deleted on the host and forgotten.
    pub fn forget(&mut self, pane: PaneId) -> Vec<Vec<u8>> {
        self.inflight.remove(&pane);
        self.ids.retain(|(p, _), _| *p != pane);
        self.numbers.retain(|(p, _), _| *p != pane);
        let gone: Vec<u32> = self
            .images
            .iter()
            .filter(|(_, img)| img.pane == pane)
            .map(|(h, _)| *h)
            .collect();
        gone.into_iter()
            .map(|h| {
                self.images.remove(&h);
                delete_on_host('I', h, None)
            })
            .collect()
    }

    /// The program's id a command names: `i`, else its number's.
    fn prog_of(&self, pane: PaneId, cmd: &Command) -> Option<u32> {
        cmd.num("i")
            .filter(|i| *i != 0)
            .or_else(|| self.numbers.get(&(pane, cmd.num("I")?)).copied())
    }

    fn next_host(&mut self) -> u32 {
        // Inside 24 bits, so the placeholder's colour holds the whole id.
        self.last = self.last % 0x00ff_ffff + 1;
        self.last
    }

    fn transmit(&mut self, pane: PaneId, mut cmd: Command, out: &mut Out) {
        let number = cmd.num("I");
        let asked = cmd.num("i").filter(|i| *i != 0);
        // An image sent by number gets an id from us, as the terminal would
        // give it one; the host's id is unique, so it serves.
        let (prog, host) = match asked {
            Some(p) => match self.host_id(pane, p) {
                Some(h) => (p, h),
                None => {
                    let h = self.next_host();
                    (p, h)
                }
            },
            None => {
                let h = self.next_host();
                (h, h)
            }
        };
        self.ids.insert((pane, prog), host);
        if let Some(n) = number {
            self.numbers.insert((pane, n), prog);
        }
        // Sent again under the same id: it replaces what was there.
        let img = self.images.entry(host).or_default();
        img.pane = pane;
        img.prog = prog;
        img.data.clear();

        let reply = (cmd.quiet() == 0 && (asked.is_some() || number.is_some()))
            .then(|| answer(&cmd, Some(prog), "OK"));
        // Shown at the host's cursor is not where the pane is: only a
        // virtual placement is kept from "transmit and display".
        if cmd.action() == 'T' && cmd.get("U") != Some("1") {
            cmd.set("a", "t");
        }
        let medium = cmd.get("t").unwrap_or("d").to_string();
        if medium != "d" {
            match read_medium(&medium, &cmd) {
                Ok(bytes) => {
                    for c in self.direct_chunks(&cmd, host, &bytes) {
                        self.send_data(host, c, out);
                    }
                    if let Some(r) = reply {
                        out.reply = r;
                    }
                }
                Err(e) => {
                    if cmd.quiet() < 2 {
                        out.reply = answer(&cmd, Some(prog), &format!("EBADF:{e}"));
                    }
                    self.images.remove(&host);
                }
            }
            return;
        }
        let more = cmd.get("m") == Some("1");
        rewrite(&mut cmd, host);
        self.send_data(host, cmd.encode(), out);
        if more {
            self.inflight.insert(pane, Inflight { host, reply });
        } else if let Some(r) = reply {
            out.reply = r;
        }
    }

    fn continue_chunk(&mut self, pane: PaneId, cmd: Command, out: &mut Out) {
        let more = cmd.get("m") == Some("1");
        let host = self.inflight[&pane].host;
        let c = Command {
            keys: vec![
                ("m".into(), if more { "1" } else { "0" }.into()),
                ("q".into(), "2".into()),
            ],
            payload: cmd.payload,
        };
        self.send_data(host, c.encode(), out);
        if !more && let Some(f) = self.inflight.remove(&pane) {
            out.reply = f.reply.unwrap_or_default();
        }
    }

    fn send_data(&mut self, host: u32, bytes: Vec<u8>, out: &mut Out) {
        if let Some(img) = self.images.get_mut(&host) {
            img.data.push(bytes.clone());
        }
        out.host.push(bytes);
    }

    /// A file's bytes as direct chunks, the first with the command's keys.
    fn direct_chunks(&self, cmd: &Command, host: u32, bytes: &[u8]) -> Vec<Vec<u8>> {
        let b64 = base64(bytes);
        let parts: Vec<&[u8]> = b64.as_bytes().chunks(CHUNK).collect();
        let n = parts.len().max(1);
        (0..n)
            .map(|i| {
                let more = if i + 1 < n { "1" } else { "0" };
                let payload = parts.get(i).map(|p| p.to_vec()).unwrap_or_default();
                if i == 0 {
                    let mut c = cmd.clone();
                    for k in ["t", "S", "O"] {
                        c.remove(k);
                    }
                    rewrite(&mut c, host);
                    c.set("m", more);
                    c.payload = payload;
                    c.encode()
                } else {
                    Command {
                        keys: vec![("m".into(), more.into()), ("q".into(), "2".into())],
                        payload,
                    }
                    .encode()
                }
            })
            .collect()
    }

    fn place(&mut self, pane: PaneId, mut cmd: Command, out: &mut Out) {
        let Some(prog) = self.prog_of(pane, &cmd) else {
            return;
        };
        let Some(host) = self.host_id(pane, prog) else {
            if cmd.quiet() < 2 {
                out.reply = answer(&cmd, Some(prog), "ENOENT:no such image");
            }
            return;
        };
        if cmd.get("U") != Some("1") {
            if cmd.quiet() < 2 {
                out.reply = answer(
                    &cmd,
                    Some(prog),
                    "EINVAL:ranma shows images by Unicode placeholders only (U=1)",
                );
            }
            return;
        }
        let reply = (cmd.quiet() == 0).then(|| answer(&cmd, Some(prog), "OK"));
        let p = cmd.num("p").unwrap_or(0);
        rewrite(&mut cmd, host);
        let bytes = cmd.encode();
        if let Some(img) = self.images.get_mut(&host) {
            img.placements.insert(p, bytes.clone());
        }
        out.host.push(bytes);
        if let Some(r) = reply {
            out.reply = r;
        }
    }

    fn delete(&mut self, pane: PaneId, cmd: Command, out: &mut Out) {
        let what = cmd.get("d").and_then(|d| d.chars().next()).unwrap_or('a');
        let free = what.is_ascii_uppercase();
        let hosts: Vec<u32> = match what.to_ascii_lowercase() {
            'a' => self
                .images
                .iter()
                .filter(|(_, img)| img.pane == pane)
                .map(|(h, _)| *h)
                .collect(),
            'i' | 'n' => self
                .prog_of(pane, &cmd)
                .and_then(|p| self.host_id(pane, p))
                .into_iter()
                .collect(),
            // By position, cell or z-index: a virtual placement has none.
            _ => Vec::new(),
        };
        let p = cmd.num("p").filter(|_| matches!(what, 'i' | 'I'));
        for h in hosts {
            out.host
                .push(delete_on_host(if free { 'I' } else { 'i' }, h, p));
            if free {
                if let Some(img) = self.images.remove(&h) {
                    self.ids.remove(&(pane, img.prog));
                    self.numbers.retain(|_, prog| *prog != img.prog);
                }
            } else if let Some(img) = self.images.get_mut(&h) {
                match p {
                    Some(p) => {
                        img.placements.remove(&p);
                    }
                    None => img.placements.clear(),
                }
            }
        }
    }

    /// Past the cap, the oldest images are no longer kept to send again;
    /// the host keeps showing them.
    fn evict(&mut self) {
        let mut total: usize = self.images.values().map(Image::bytes).sum();
        while total > STORE_CAP {
            let busy: Vec<u32> = self.inflight.values().map(|f| f.host).collect();
            let Some(oldest) = self.images.keys().copied().find(|h| !busy.contains(h)) else {
                return;
            };
            if let Some(img) = self.images.remove(&oldest) {
                total -= img.bytes();
            }
        }
    }
}

/// The command's id keys swapped for the host's id, and the host told to
/// keep quiet: its answers would arrive at ranma as keys.
fn rewrite(cmd: &mut Command, host: u32) {
    cmd.remove("I");
    cmd.set("i", host);
    cmd.set("q", 2);
}

fn delete_on_host(d: char, host: u32, p: Option<u32>) -> Vec<u8> {
    let mut c = Command::default();
    c.set("a", "d");
    c.set("d", d);
    c.set("i", host);
    if let Some(p) = p {
        c.set("p", p);
    }
    c.set("q", 2);
    c.encode()
}

/// An answer to the program, naming what it named.
fn answer(cmd: &Command, prog: Option<u32>, msg: &str) -> Vec<u8> {
    let mut keys = Vec::new();
    if let Some(i) = prog.or_else(|| cmd.num("i")) {
        keys.push(format!("i={i}"));
    }
    for k in ["I", "p"] {
        if let Some(v) = cmd.get(k) {
            keys.push(format!("{k}={v}"));
        }
    }
    format!("\x1b_G{};{msg}\x1b\\", keys.join(",")).into_bytes()
}

/// The bytes a file, temporary file or shared memory object holds, read
/// where the program runs. The program's path is base64 in the payload.
fn read_medium(medium: &str, cmd: &Command) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let raw = unbase64(&cmd.payload).ok_or("the path is not base64")?;
    let name = String::from_utf8(raw).map_err(|_| "the path is not UTF-8")?;
    let path = match medium {
        "f" | "t" => std::path::PathBuf::from(&name),
        "s" => std::path::Path::new("/dev/shm").join(name.trim_start_matches('/')),
        other => return Err(format!("unknown transmission medium {other}")),
    };
    // As kitty: nothing that is a device, a process or the kernel.
    if medium != "s"
        && ["/proc/", "/sys/", "/dev/"]
            .iter()
            .any(|p| path.starts_with(p))
    {
        return Err("not a file ranma reads".into());
    }
    if medium == "t" && !name.contains("tty-graphics-protocol") {
        return Err("a temporary file's name must contain tty-graphics-protocol".into());
    }
    let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let meta = f.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if let Some(off) = cmd.num("O") {
        f.seek(SeekFrom::Start(off as u64))
            .map_err(|e| e.to_string())?;
    }
    let size = cmd
        .num("S")
        .map(u64::from)
        .unwrap_or(FILE_CAP)
        .min(FILE_CAP);
    let mut bytes = Vec::new();
    f.take(size)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    // The protocol hands these over: the terminal removes them once read.
    if medium != "f" {
        let _ = std::fs::remove_file(&path);
    }
    Ok(bytes)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn unbase64(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for &c in input {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' => continue,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The image id a placeholder cell names: its colour (24 bits, or a palette
/// index) and the high byte from its third combining mark.
pub fn placeholder_id(fg: Fg, marks: &[char]) -> Option<u32> {
    let low = match fg {
        Fg::Rgb(r, g, b) => (r as u32) << 16 | (g as u32) << 8 | b as u32,
        Fg::Indexed(i) => i as u32,
        Fg::Default => return None,
    };
    let high = match marks.get(2) {
        Some(c) => DIACRITICS.iter().position(|d| *d == *c as u32)? as u32,
        None => 0,
    };
    Some(high << 24 | low)
}

/// A placeholder's foreground as the grid has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fg {
    Rgb(u8, u8, u8),
    Indexed(u8),
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    fn text(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    #[test]
    fn base64_both_ways() {
        for s in ["", "a", "ab", "abc", "abcd", "hello world"] {
            assert_eq!(
                unbase64(base64(s.as_bytes()).as_bytes()).unwrap(),
                s.as_bytes()
            );
        }
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn ids_are_the_hosts_per_pane_and_the_host_keeps_quiet() {
        let mut g = Graphics::default();
        let a = g.handle(1, &cmd("Ga=T,U=1,i=7,f=100,m=1;AAAA"), true);
        let b = g.handle(2, &cmd("Ga=t,i=7,f=100;BBBB"), true);
        let (ha, hb) = (g.host_id(1, 7).unwrap(), g.host_id(2, 7).unwrap());
        assert_ne!(ha, hb, "the same id in two panes is two images");
        assert_eq!(
            text(&a.host[0]),
            format!("\x1b_Ga=T,U=1,i={ha},f=100,m=1,q=2;AAAA\x1b\\")
        );
        assert!(a.reply.is_empty(), "the answer waits for the last chunk");
        assert_eq!(text(&b.reply), "\x1b_Gi=7;OK\x1b\\");
        // The rest of pane 1's image: no id, no action.
        let c = g.handle(1, &cmd("Gm=0;CCCC"), true);
        assert_eq!(text(&c.host[0]), "\x1b_Gm=0,q=2;CCCC\x1b\\");
        assert_eq!(text(&c.reply), "\x1b_Gi=7;OK\x1b\\");
        assert_eq!(g.replay().len(), 3);
    }

    #[test]
    fn a_query_is_answered_here_and_never_reaches_the_host() {
        let mut g = Graphics::default();
        let q = g.handle(1, &cmd("Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA"), true);
        assert!(q.host.is_empty());
        assert_eq!(text(&q.reply), "\x1b_Gi=31;OK\x1b\\");
        let no = g.handle(1, &cmd("Gi=31,a=q;AAAA"), false);
        assert!(text(&no.reply).contains("ENOTSUPPORTED"));
    }

    #[test]
    fn only_virtual_placements_are_kept_and_deletes_follow_the_ids() {
        let mut g = Graphics::default();
        g.handle(1, &cmd("Ga=t,i=5,q=2;AAAA"), true);
        let h = g.host_id(1, 5).unwrap();
        let direct = g.handle(1, &cmd("Ga=p,i=5"), true);
        assert!(direct.host.is_empty());
        assert!(text(&direct.reply).contains("EINVAL"));
        let virt = g.handle(1, &cmd("Ga=p,U=1,i=5,c=10,r=4"), true);
        assert_eq!(
            text(&virt.host[0]),
            format!("\x1b_Ga=p,U=1,i={h},c=10,r=4,q=2\x1b\\")
        );
        // "Transmit and display" without U=1 only transmits.
        let t = g.handle(1, &cmd("Ga=T,i=6,q=2;AAAA"), true);
        assert!(text(&t.host[0]).starts_with("\x1b_Ga=t,"));
        let del = g.handle(1, &cmd("Ga=d,d=I,i=5"), true);
        assert_eq!(text(&del.host[0]), format!("\x1b_Ga=d,d=I,i={h},q=2\x1b\\"));
        assert_eq!(g.host_id(1, 5), None);
        // Deleting all goes no further than the pane's own.
        g.handle(2, &cmd("Ga=t,i=1,q=2;AAAA"), true);
        let all = g.handle(1, &cmd("Ga=d"), true);
        assert_eq!(all.host.len(), 1, "pane 1 has one image left");
        assert_eq!(g.forget(2).len(), 1);
        assert!(g.host_id(2, 1).is_none());
    }

    #[test]
    fn a_file_is_read_here_and_sent_as_data() {
        let dir = std::env::temp_dir().join(format!("ranma-gfx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img.rgb");
        std::fs::write(&path, vec![7u8; 5000]).unwrap();
        let mut g = Graphics::default();
        let c = format!(
            "Ga=t,t=f,i=3,f=24,s=10,v=10;{}",
            base64(path.to_str().unwrap().as_bytes())
        );
        let out = g.handle(1, c.as_bytes(), true);
        let h = g.host_id(1, 3).unwrap();
        // 5000 bytes are 6668 of base64: two chunks.
        assert_eq!(out.host.len(), 2);
        assert!(
            text(&out.host[0]).starts_with(&format!("\x1b_Ga=t,i={h},f=24,s=10,v=10,q=2,m=1;"))
        );
        assert!(text(&out.host[1]).starts_with("\x1b_Gm=0,q=2;"));
        assert_eq!(text(&out.reply), "\x1b_Gi=3;OK\x1b\\");
        assert!(path.exists(), "a plain file is left where it is");
        let bad = format!("Ga=t,t=f,i=4;{}", base64(b"/proc/self/environ"));
        assert!(text(&g.handle(1, bad.as_bytes(), true).reply).contains("EBADF"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_placeholder_names_its_image() {
        let mark = |i: usize| char::from_u32(DIACRITICS[i]).unwrap();
        assert_eq!(
            placeholder_id(Fg::Rgb(0, 1, 2), &[mark(0), mark(0)]),
            Some(258)
        );
        assert_eq!(placeholder_id(Fg::Indexed(42), &[]), Some(42));
        assert_eq!(
            placeholder_id(Fg::Rgb(0, 0, 1), &[mark(0), mark(0), mark(2)]),
            Some(2 << 24 | 1)
        );
        assert_eq!(placeholder_id(Fg::Default, &[]), None);
    }
}
