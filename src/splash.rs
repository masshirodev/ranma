//! The empty workspace: the logo, and how to start (the `splash` setting).
//!
//! A workspace with no pane used to be a blank screen that answered Enter
//! with a shell, which nothing on it said. This is what it says, centred:
//! the logo, then the keys. It shrinks to fit: the name in plain letters on a
//! screen too small for the logo, and nothing on one too small for the keys.

use unicode_width::UnicodeWidthStr;

/// "RANMA" in figlet's Merlin1 (by LG Beard), at its default spacing.
pub const LOGO: [&str; 7] = [
    r#"  _______        __      _____  ___   ___      ___       __      "#,
    r#" /"      \      /""\    (\"   \|"  \ |"  \    /"  |     /""\     "#,
    r#"|:        |    /    \   |.\\   \    | \   \  //   |    /    \    "#,
    r#"|_____/   )   /' /\  \  |: \.   \\  | /\\  \/.    |   /' /\  \   "#,
    r#" //      /   //  __'  \ |.  \    \. ||: \.        |  //  __'  \  "#,
    r#"|:  __   \  /   /  \\  \|    \    \ ||.  \    /:  | /   /  \\  \ "#,
    r#"|__|  \___)(___/    \___)\___|\____\)|___|\__/|___|(___/    \___)"#,
];

/// What a piece of the splash is, for its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The logo, or the name standing in for it.
    Logo,
    /// A key to press.
    Key,
    /// What the key does.
    Text,
}

pub type Line = Vec<(String, Part)>;

/// The splash's lines for a `w` x `h` area, each a row of pieces; the caller
/// centres the block. `keys` are (key, what it does), in order.
pub fn lines(w: u16, h: u16, keys: &[(String, &str)]) -> Vec<Line> {
    let (w, h) = (w as usize, h as usize);
    let tips = tips(keys);
    let tips_w = tips.iter().map(line_width).max().unwrap_or(0);
    let logo_w = LOGO.iter().map(|l| l.width()).max().unwrap_or(0);
    // The logo and the keys are separated by one blank row.
    let with = |head: Vec<Line>| {
        let mut out = head;
        if !tips.is_empty() {
            out.push(Vec::new());
            out.extend(tips.iter().cloned());
        }
        out
    };
    if w >= logo_w.max(tips_w) && h >= LOGO.len() + 1 + tips.len() {
        return with(
            LOGO.iter()
                .map(|l| vec![(l.to_string(), Part::Logo)])
                .collect(),
        );
    }
    if w >= tips_w.max(5) && h >= 2 + tips.len() {
        return with(vec![vec![("ranma".to_string(), Part::Logo)]]);
    }
    Vec::new()
}

/// The keys alone, right-aligned so what they do starts in one column: the
/// splash under a background, where the art stands in for the logo.
pub fn tips(keys: &[(String, &str)]) -> Vec<Line> {
    let key_w = keys.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    keys.iter()
        .map(|(k, what)| {
            vec![
                (format!("{k:>key_w$}"), Part::Key),
                (format!("   {what}"), Part::Text),
            ]
        })
        .collect()
}

pub fn line_width(l: &Line) -> usize {
    l.iter().map(|(s, _)| s.width()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<(String, &'static str)> {
        vec![
            ("Enter".to_string(), "open a shell"),
            ("ctrl+b ?".to_string(), "every key"),
        ]
    }

    fn text(l: &Line) -> String {
        l.iter().map(|(s, _)| s.as_str()).collect()
    }

    #[test]
    fn the_logo_and_the_keys_lined_up_under_it() {
        let l = lines(120, 30, &keys());
        assert_eq!(l.len(), LOGO.len() + 1 + 2);
        assert_eq!(text(&l[0]), LOGO[0]);
        assert!(l[LOGO.len()].is_empty(), "a blank row between");
        // The keys are right-aligned, so what they do starts in one column.
        assert_eq!(text(&l[8]), "   Enter   open a shell");
        assert_eq!(text(&l[9]), "ctrl+b ?   every key");
        assert_eq!(l[8][0].1, Part::Key);
        assert_eq!(l[8][1].1, Part::Text);
    }

    #[test]
    fn a_small_screen_gets_the_name_then_nothing() {
        let narrow = lines(40, 30, &keys());
        assert_eq!(text(&narrow[0]), "ranma");
        assert_eq!(narrow.len(), 4);
        let short = lines(120, 6, &keys());
        assert_eq!(text(&short[0]), "ranma", "too short for the logo");
        assert!(lines(10, 30, &keys()).is_empty(), "too narrow for the keys");
        assert!(lines(120, 3, &keys()).is_empty(), "too short for the keys");
    }

    #[test]
    fn no_keys_is_the_logo_alone() {
        let l = lines(120, 30, &[]);
        assert_eq!(l.len(), LOGO.len());
    }

    #[test]
    fn the_logo_is_all_one_width() {
        // Centring treats it as a block; a ragged line would shift by itself.
        let w = LOGO[0].width();
        assert!(LOGO.iter().all(|l| l.width() == w), "{w}");
    }
}
