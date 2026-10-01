//! Terminal resizes, read straight off SIGWINCH instead of from crossterm.
//!
//! crossterm loses resizes. When one wakeup of its poll reports both input on
//! the terminal and the signal, it returns the first key it parses and drops
//! the rest of that batch, and mio's epoll is edge-triggered, so the signal is
//! never reported again: the resize is lost until the next one. A nested ranma
//! over SSH is exactly that case. Its outer framing the pane as the scratchpad
//! opens resizes it and reports focus to it in the same instant, both arrive
//! in one burst, and the inner went on drawing at the old size into a grid
//! that had changed, leaving both sizes' text overlaid.
//!
//! signal-hook's iterator reads its own pipe with a blocking read, which never
//! misses a signal. The input threads ignore crossterm's `Resize` and take
//! these instead.

use std::io;

/// Call `on_resize` with the terminal's size after every SIGWINCH, from a
/// thread of its own, until it returns false. `size` reads the size (a
/// parameter so a test can do without a terminal).
pub fn spawn(
    size: fn() -> io::Result<(u16, u16)>,
    mut on_resize: impl FnMut(u16, u16) -> bool + Send + 'static,
) -> io::Result<()> {
    let mut signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGWINCH])?;
    std::thread::Builder::new()
        .name("winch".into())
        .spawn(move || {
            for _ in signals.forever() {
                // Several resizes before this wakes are one: the size now is
                // the one that matters.
                let Ok((cols, rows)) = size() else { continue };
                if !on_resize(cols, rows) {
                    return;
                }
            }
        })?;
    Ok(())
}

/// crossterm's own resize: dropped by the input threads, since [`spawn`]
/// reports every one and this one only sometimes.
pub fn from_crossterm(ev: &crossterm::event::Event) -> bool {
    matches!(ev, crossterm::event::Event::Resize(..))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn every_sigwinch_is_a_resize_at_the_size_then() {
        let (tx, rx) = mpsc::channel();
        spawn(|| Ok((58, 27)), move |c, r| tx.send((c, r)).is_ok()).unwrap();
        // SIGWINCH is ignored by default, so raising it in the test process
        // disturbs nothing else.
        unsafe { libc::raise(libc::SIGWINCH) };
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok((58, 27)));
        unsafe { libc::raise(libc::SIGWINCH) };
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok((58, 27)),
            "and the next one too"
        );
    }

    #[test]
    fn crossterms_own_resize_is_the_one_dropped() {
        use crossterm::event::{Event, KeyCode, KeyEvent};
        assert!(from_crossterm(&Event::Resize(80, 24)));
        assert!(!from_crossterm(&Event::Key(KeyEvent::from(KeyCode::Char(
            'a'
        )))));
        assert!(!from_crossterm(&Event::FocusLost));
    }
}
