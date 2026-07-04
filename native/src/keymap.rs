//! HID keyboard/keypad usage (scancode) → human-readable key mapping.
//!
//! Ported from ../native-key-logger. Classifies each entry so the callback
//! logic needs no magic-number range checks.

/// How a key should be treated when logged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyKind {
    /// A normal key: write [`KeyEntry::base`] (or [`KeyEntry::shifted`] when
    /// caps-lock is latched).
    Printable,
    /// A modifier (control/shift/alt/cmd): wrapped as `name(` on press and `)`
    /// on release.
    Modifier,
    /// The caps-lock key: toggles the software latch, writes nothing.
    CapsLock,
}

/// A single mapped key.
#[derive(Clone, Copy, Debug)]
pub struct KeyEntry {
    /// Representation when caps-lock is off / key is unshifted.
    pub base: &'static str,
    /// Representation when caps-lock is on / key is shifted.
    pub shifted: &'static str,
    /// Classification driving how the key is logged.
    pub kind: KeyKind,
}

/// Look up a HID usage (scancode). Returns `None` for usages outside the known
/// table.
pub fn lookup(usage: u32) -> Option<KeyEntry> {
    use KeyKind::{CapsLock, Modifier, Printable};

    let p = |base, shifted| {
        Some(KeyEntry {
            base,
            shifted,
            kind: Printable,
        })
    };
    let m = |name| {
        Some(KeyEntry {
            base: name,
            shifted: name,
            kind: Modifier,
        })
    };

    match usage {
        4 => p("a", "A"),
        5 => p("b", "B"),
        6 => p("c", "C"),
        7 => p("d", "D"),
        8 => p("e", "E"),
        9 => p("f", "F"),
        10 => p("g", "G"),
        11 => p("h", "H"),
        12 => p("i", "I"),
        13 => p("j", "J"),
        14 => p("k", "K"),
        15 => p("l", "L"),
        16 => p("m", "M"),
        17 => p("n", "N"),
        18 => p("o", "O"),
        19 => p("p", "P"),
        20 => p("q", "Q"),
        21 => p("r", "R"),
        22 => p("s", "S"),
        23 => p("t", "T"),
        24 => p("u", "U"),
        25 => p("v", "V"),
        26 => p("w", "W"),
        27 => p("x", "X"),
        28 => p("y", "Y"),
        29 => p("z", "Z"),
        30 => p("1", "!"),
        31 => p("2", "@"),
        32 => p("3", "#"),
        33 => p("4", "$"),
        34 => p("5", "%"),
        35 => p("6", "^"),
        36 => p("7", "&"),
        37 => p("8", "*"),
        38 => p("9", "("),
        39 => p("0", ")"),
        40 => p("\n", "\n"),
        41 => p("\\ESCAPE", "\\ESCAPE"),
        42 => p("\\DELETE|BACKSPACE", "\\DELETE|BACKSPACE"),
        43 => p("\\TAB", "\\TAB"),
        44 => p(" ", " "),
        45 => p("-", "_"),
        46 => p("=", "+"),
        47 => p("[", "{"),
        48 => p("]", "}"),
        49 => p("\\", "|"),
        50 => p("", ""),
        51 => p(";", ":"),
        52 => p("'", "\""),
        53 => p("`", "~"),
        54 => p(",", "<"),
        55 => p(".", ">"),
        56 => p("/", "?"),
        57 => Some(KeyEntry {
            base: "\\CAPSLOCK",
            shifted: "\\CAPSLOCK",
            kind: CapsLock,
        }),
        58 => p("\\F1", "\\F1"),
        59 => p("\\F2", "\\F2"),
        60 => p("\\F3", "\\F3"),
        61 => p("\\F4", "\\F4"),
        62 => p("\\F5", "\\F5"),
        63 => p("\\F6", "\\F6"),
        64 => p("\\F7", "\\F7"),
        65 => p("\\F8", "\\F8"),
        66 => p("\\F9", "\\F9"),
        67 => p("\\F10", "\\F10"),
        68 => p("\\F11", "\\F11"),
        69 => p("\\F12", "\\F12"),
        70 => p("\\PRINTSCREEN", "\\PRINTSCREEN"),
        71 => p("\\SCROLL-LOCK", "\\SCROLL-LOCK"),
        72 => p("\\PAUSE", "\\PAUSE"),
        73 => p("\\INSERT", "\\INSERT"),
        74 => p("\\HOME", "\\HOME"),
        75 => p("\\PAGEUP", "\\PAGEUP"),
        76 => p("\\DELETE-FORWARD", "\\DELETE-FORWARD"),
        77 => p("\\END", "\\END"),
        78 => p("\\PAGEDOWN", "\\PAGEDOWN"),
        79 => p("\\RIGHTARROW", "\\RIGHTARROW"),
        80 => p("\\LEFTARROW", "\\LEFTARROW"),
        81 => p("\\DOWNARROW", "\\DOWNARROW"),
        82 => p("\\UPARROW", "\\UPARROW"),
        83 => p("\\NUMLOCK", "\\CLEAR"),
        84 => p("/", "/"),
        85 => p("*", "*"),
        86 => p("-", "-"),
        87 => p("+", "+"),
        88 => p("\\ENTER", "\\ENTER"),
        89 => p("1", "\\END"),
        90 => p("2", "\\DOWNARROW"),
        91 => p("3", "\\PAGEDOWN"),
        92 => p("4", "\\LEFTARROW"),
        93 => p("5", "5"),
        94 => p("6", "\\RIGHTARROW"),
        95 => p("7", "\\HOME"),
        96 => p("8", "\\UPARROW"),
        97 => p("9", "\\PAGEUP"),
        98 => p("0", "\\INSERT"),
        99 => p(".", "\\DELETE"),
        100 => p("", ""),
        224 => m("\\LC"),   // left control
        225 => m("\\LS"),   // left shift
        226 => m("\\LA"),   // left alt
        227 => m("\\LCMD"), // left command
        228 => m("\\RC"),   // right control
        229 => m("\\RS"),   // right shift
        230 => m("\\RA"),   // right alt
        231 => m("\\RCMD"), // right command
        _ => None,
    }
}
