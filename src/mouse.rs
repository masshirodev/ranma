//! Mouse binds (`ranma.mouse`, DESIGN.md "Mouse binds"): a button, the wheel
//! or a double click, with modifiers, on a target: a pane's text, a pane's
//! border, or the background where no pane is.
//!
//! Spelled `[ctrl+][alt+][shift+]EVENT@TARGET` and parsed as strictly as a
//! key: an unknown event, target or modifier is an error naming it, and so is
//! a bind that would take what already belongs to someone (a plain click in
//! a program's text, the left press that drags a border).

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    Left,
    Middle,
    Right,
    /// A second left press on the same cell soon after the first.
    Double,
    WheelUp,
    WheelDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// A pane's text.
    Pane,
    /// A pane's border (its title edge included).
    Border,
    /// Where no pane is: the gaps, an empty workspace, around floats.
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spec {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub event: Event,
    pub target: Target,
}

const EVENTS: &str = "left, middle, right, double, wheel_up, wheel_down";
const TARGETS: &str = "pane, border, background";

impl FromStr for Spec {
    type Err = String;

    fn from_str(s: &str) -> Result<Spec, String> {
        let (chord, target) = s
            .split_once('@')
            .ok_or_else(|| format!("`{s}`: give a target after @ ({TARGETS})"))?;
        let target = match target {
            "pane" => Target::Pane,
            "border" => Target::Border,
            "background" => Target::Background,
            t => return Err(format!("`{s}`: no target `{t}` ({TARGETS})")),
        };
        let mut parts: Vec<&str> = chord.split('+').collect();
        let event = parts.pop().unwrap_or("");
        let event = match event {
            "left" => Event::Left,
            "middle" => Event::Middle,
            "right" => Event::Right,
            "double" => Event::Double,
            "wheel_up" => Event::WheelUp,
            "wheel_down" => Event::WheelDown,
            e => return Err(format!("`{s}`: no mouse event `{e}` ({EVENTS})")),
        };
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        for m in parts {
            let slot = match m {
                "ctrl" => &mut ctrl,
                "alt" => &mut alt,
                "shift" => &mut shift,
                m => return Err(format!("`{s}`: no modifier `{m}` (ctrl, alt, shift)")),
            };
            if *slot {
                return Err(format!("`{s}`: `{m}` twice"));
            }
            *slot = true;
        }
        let spec = Spec {
            ctrl,
            alt,
            shift,
            event,
            target,
        };
        // A pane's plain clicks are its program's (and focus's); Shift is the
        // terminal's own selection past a program's mouse capture.
        if target == Target::Pane && !(ctrl || alt) {
            return Err(format!(
                "`{s}`: a bind in a pane's text needs ctrl or alt: plain clicks and the wheel there are its program's"
            ));
        }
        if target == Target::Border && event == Event::Left && !(ctrl || alt || shift) {
            return Err(format!(
                "`{s}`: a plain left press on a border moves or resizes the pane; give a modifier, or use double"
            ));
        }
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse_strictly_and_leave_others_what_is_theirs() {
        let s: Spec = "alt+middle@pane".parse().unwrap();
        assert_eq!(
            (s.alt, s.ctrl, s.event, s.target),
            (true, false, Event::Middle, Target::Pane)
        );
        assert!("double@border".parse::<Spec>().is_ok());
        assert!("wheel_up@background".parse::<Spec>().is_ok());
        assert!("ctrl+shift+right@border".parse::<Spec>().is_ok());
        let err = |s: &str| s.parse::<Spec>().unwrap_err();
        assert!(err("middle@pane").contains("needs ctrl or alt"));
        assert!(err("shift+left@pane").contains("needs ctrl or alt"));
        assert!(err("left@border").contains("moves or resizes"));
        assert!(err("alt+middle").contains("give a target"));
        assert!(err("alt+mid@pane").contains("no mouse event `mid`"));
        assert!(err("alt+middle@bar").contains("no target `bar`"));
        assert!(err("super+middle@pane").contains("no modifier `super`"));
        assert!(err("alt+alt+middle@pane").contains("twice"));
    }
}
