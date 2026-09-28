//! Key chords as the config spells them: `"ctrl+b"`, `"shift+right"`, `"alt+return"`, `"1"`.
//!
//! Parsing is strict on purpose. A typo like `"ctlr+b"` must fail at config load with a
//! message naming it, not silently become a bind that never fires.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Return,
    Tab,
    Backspace,
    Escape,
    Space,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Mods,
    pub key: Key,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChordError {
    #[error("empty key chord")]
    Empty,
    #[error("unknown modifier `{0}` (expected ctrl, alt, shift or super)")]
    UnknownModifier(String),
    #[error("unknown key `{0}`")]
    UnknownKey(String),
    #[error("modifier `{0}` given twice")]
    DuplicateModifier(String),
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ChordError::Empty);
        }
        // "+" alone, or "ctrl++", means the plus key itself.
        let (mod_part, key_part) = match s.strip_suffix("++") {
            Some(rest) => (Some(rest), "+"),
            None if s == "+" => (None, "+"),
            None => match s.rsplit_once('+') {
                Some((m, k)) => (Some(m), k),
                None => (None, s),
            },
        };

        let mut mods = Mods::default();
        if let Some(mod_part) = mod_part {
            for m in mod_part.split('+') {
                let slot = match m.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => &mut mods.ctrl,
                    "alt" | "meta" | "opt" | "option" => &mut mods.alt,
                    "shift" => &mut mods.shift,
                    "super" | "mod4" | "win" | "cmd" => &mut mods.super_,
                    "" => return Err(ChordError::Empty),
                    _ => return Err(ChordError::UnknownModifier(m.to_string())),
                };
                if *slot {
                    return Err(ChordError::DuplicateModifier(m.to_string()));
                }
                *slot = true;
            }
        }

        Ok(Chord {
            mods,
            key: parse_key(key_part)?,
        })
    }
}

fn parse_key(s: &str) -> Result<Key, ChordError> {
    let mut chars = s.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        // Letters are stored lowercase; Shift is a modifier, not a different letter.
        return Ok(Key::Char(c.to_ascii_lowercase()));
    }
    let lower = s.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "return" | "enter" | "ret" => Key::Return,
        "tab" => Key::Tab,
        "backspace" | "bs" => Key::Backspace,
        "escape" | "esc" => Key::Escape,
        "space" | "spc" => Key::Space,
        "delete" | "del" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "plus" => Key::Char('+'),
        "minus" => Key::Char('-'),
        "comma" => Key::Char(','),
        "period" => Key::Char('.'),
        "slash" => Key::Char('/'),
        _ => {
            if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
                && (1..=24).contains(&n)
            {
                return Ok(Key::F(n));
            }
            return Err(ChordError::UnknownKey(s.to_string()));
        }
    })
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.mods;
        for (on, name) in [
            (m.ctrl, "ctrl"),
            (m.alt, "alt"),
            (m.shift, "shift"),
            (m.super_, "super"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        match self.key {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "f{n}"),
            other => write!(f, "{}", format!("{other:?}").to_ascii_lowercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Chord {
        s.parse().unwrap()
    }

    #[test]
    fn parses_plain_and_modified_keys() {
        assert_eq!(c("t").key, Key::Char('t'));
        let chord = c("ctrl+b");
        assert!(chord.mods.ctrl && !chord.mods.alt);
        assert_eq!(chord.key, Key::Char('b'));
        let chord = c("Ctrl+Shift+Right");
        assert!(chord.mods.ctrl && chord.mods.shift);
        assert_eq!(chord.key, Key::Right);
    }

    #[test]
    fn letters_are_case_insensitive() {
        assert_eq!(c("B"), c("b"));
    }

    #[test]
    fn plus_key_is_expressible() {
        assert_eq!(c("+").key, Key::Char('+'));
        let chord = c("ctrl++");
        assert!(chord.mods.ctrl);
        assert_eq!(chord.key, Key::Char('+'));
    }

    #[test]
    fn rejects_typos() {
        assert_eq!(
            "ctlr+b".parse::<Chord>(),
            Err(ChordError::UnknownModifier("ctlr".into()))
        );
        assert_eq!(
            "ctrl+retrun".parse::<Chord>(),
            Err(ChordError::UnknownKey("retrun".into()))
        );
        assert_eq!(
            "ctrl+ctrl+b".parse::<Chord>(),
            Err(ChordError::DuplicateModifier("ctrl".into()))
        );
        assert_eq!("".parse::<Chord>(), Err(ChordError::Empty));
    }

    #[test]
    fn function_keys() {
        assert_eq!(c("f5").key, Key::F(5));
        assert!("f25".parse::<Chord>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "ctrl+b",
            "shift+right",
            "alt+return",
            "1",
            "ctrl+alt+shift+f12",
        ] {
            assert_eq!(c(&c(s).to_string()), c(s), "{s}");
        }
    }
}
