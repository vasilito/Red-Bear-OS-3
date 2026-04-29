use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

mod keymaps {
    pub static US: [(u8, [char; 2]); 53] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['1', '!']),
        (orbclient::K_2, ['2', '@']),
        (orbclient::K_3, ['3', '#']),
        (orbclient::K_4, ['4', '$']),
        (orbclient::K_5, ['5', '%']),
        (orbclient::K_6, ['6', '^']),
        (orbclient::K_7, ['7', '&']),
        (orbclient::K_8, ['8', '*']),
        (orbclient::K_9, ['9', '(']),
        (orbclient::K_0, ['0', ')']),
        (orbclient::K_MINUS, ['-', '_']),
        (orbclient::K_EQUALS, ['=', '+']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['q', 'Q']),
        (orbclient::K_W, ['w', 'W']),
        (orbclient::K_E, ['e', 'E']),
        (orbclient::K_R, ['r', 'R']),
        (orbclient::K_T, ['t', 'T']),
        (orbclient::K_Y, ['y', 'Y']),
        (orbclient::K_U, ['u', 'U']),
        (orbclient::K_I, ['i', 'I']),
        (orbclient::K_O, ['o', 'O']),
        (orbclient::K_P, ['p', 'P']),
        (orbclient::K_BRACE_OPEN, ['[', '{']),
        (orbclient::K_BRACE_CLOSE, [']', '}']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\0', '\0']),
        (orbclient::K_A, ['a', 'A']),
        (orbclient::K_S, ['s', 'S']),
        (orbclient::K_D, ['d', 'D']),
        (orbclient::K_F, ['f', 'F']),
        (orbclient::K_G, ['g', 'G']),
        (orbclient::K_H, ['h', 'H']),
        (orbclient::K_J, ['j', 'J']),
        (orbclient::K_K, ['k', 'K']),
        (orbclient::K_L, ['l', 'L']),
        (orbclient::K_SEMICOLON, [';', ':']),
        (orbclient::K_QUOTE, ['\'', '"']),
        (orbclient::K_TICK, ['`', '~']),
        (orbclient::K_BACKSLASH, ['\\', '|']),
        (orbclient::K_Z, ['z', 'Z']),
        (orbclient::K_X, ['x', 'X']),
        (orbclient::K_C, ['c', 'C']),
        (orbclient::K_V, ['v', 'V']),
        (orbclient::K_B, ['b', 'B']),
        (orbclient::K_N, ['n', 'N']),
        (orbclient::K_M, ['m', 'M']),
        (orbclient::K_COMMA, [',', '<']),
        (orbclient::K_PERIOD, ['.', '>']),
        (orbclient::K_SLASH, ['/', '?']),
        (orbclient::K_SPACE, [' ', ' ']),
    ];

    pub static GB: [(u8, [char; 2]); 54] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['1', '!']),
        (orbclient::K_2, ['2', '"']),
        (orbclient::K_3, ['3', '£']),
        (orbclient::K_4, ['4', '$']),
        (orbclient::K_5, ['5', '%']),
        (orbclient::K_6, ['6', '^']),
        (orbclient::K_7, ['7', '&']),
        (orbclient::K_8, ['8', '*']),
        (orbclient::K_9, ['9', '(']),
        (orbclient::K_0, ['0', ')']),
        (orbclient::K_MINUS, ['-', '_']),
        (orbclient::K_EQUALS, ['=', '+']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['q', 'Q']),
        (orbclient::K_W, ['w', 'W']),
        (orbclient::K_E, ['e', 'E']),
        (orbclient::K_R, ['r', 'R']),
        (orbclient::K_T, ['t', 'T']),
        (orbclient::K_Y, ['y', 'Y']),
        (orbclient::K_U, ['u', 'U']),
        (orbclient::K_I, ['i', 'I']),
        (orbclient::K_O, ['o', 'O']),
        (orbclient::K_P, ['p', 'P']),
        (orbclient::K_BRACE_OPEN, ['[', '{']),
        (orbclient::K_BRACE_CLOSE, [']', '}']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\0', '\0']),
        (orbclient::K_A, ['a', 'A']),
        (orbclient::K_S, ['s', 'S']),
        (orbclient::K_D, ['d', 'D']),
        (orbclient::K_F, ['f', 'F']),
        (orbclient::K_G, ['g', 'G']),
        (orbclient::K_H, ['h', 'H']),
        (orbclient::K_J, ['j', 'J']),
        (orbclient::K_K, ['k', 'K']),
        (orbclient::K_L, ['l', 'L']),
        (orbclient::K_SEMICOLON, [';', ':']),
        (orbclient::K_QUOTE, ['\'', '@']),
        (orbclient::K_TICK, ['`', '¬']),
        (orbclient::K_BACKSLASH, ['#', '~']),
        (orbclient::K_Z, ['z', 'Z']),
        (orbclient::K_X, ['x', 'X']),
        (orbclient::K_C, ['c', 'C']),
        (orbclient::K_V, ['v', 'V']),
        (orbclient::K_B, ['b', 'B']),
        (orbclient::K_N, ['n', 'N']),
        (orbclient::K_M, ['m', 'M']),
        (orbclient::K_COMMA, [',', '<']),
        (orbclient::K_PERIOD, ['.', '>']),
        (orbclient::K_SLASH, ['/', '?']),
        (orbclient::K_SPACE, [' ', ' ']),
        // UK Backslash, doesn't exist on US keyboard
        (0x56, ['\\', '|']),
    ];

    pub static DVORAK: [(u8, [char; 2]); 53] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['1', '!']),
        (orbclient::K_2, ['2', '@']),
        (orbclient::K_3, ['3', '#']),
        (orbclient::K_4, ['4', '$']),
        (orbclient::K_5, ['5', '%']),
        (orbclient::K_6, ['6', '^']),
        (orbclient::K_7, ['7', '&']),
        (orbclient::K_8, ['8', '*']),
        (orbclient::K_9, ['9', '(']),
        (orbclient::K_0, ['0', ')']),
        (orbclient::K_MINUS, ['[', '{']),
        (orbclient::K_EQUALS, [']', '}']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['\'', '"']),
        (orbclient::K_W, [',', '<']),
        (orbclient::K_E, ['.', '>']),
        (orbclient::K_R, ['p', 'P']),
        (orbclient::K_T, ['y', 'Y']),
        (orbclient::K_Y, ['f', 'F']),
        (orbclient::K_U, ['g', 'G']),
        (orbclient::K_I, ['c', 'C']),
        (orbclient::K_O, ['r', 'R']),
        (orbclient::K_P, ['l', 'L']),
        (orbclient::K_BRACE_OPEN, ['/', '?']),
        (orbclient::K_BRACE_CLOSE, ['=', '+']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\0', '\0']),
        (orbclient::K_A, ['a', 'A']),
        (orbclient::K_S, ['o', 'O']),
        (orbclient::K_D, ['e', 'E']),
        (orbclient::K_F, ['u', 'U']),
        (orbclient::K_G, ['i', 'I']),
        (orbclient::K_H, ['d', 'D']),
        (orbclient::K_J, ['h', 'H']),
        (orbclient::K_K, ['t', 'T']),
        (orbclient::K_L, ['n', 'N']),
        (orbclient::K_SEMICOLON, ['s', 'S']),
        (orbclient::K_QUOTE, ['-', '_']),
        (orbclient::K_TICK, ['`', '~']),
        (orbclient::K_BACKSLASH, ['\\', '|']),
        (orbclient::K_Z, [';', ':']),
        (orbclient::K_X, ['q', 'Q']),
        (orbclient::K_C, ['j', 'J']),
        (orbclient::K_V, ['k', 'K']),
        (orbclient::K_B, ['x', 'X']),
        (orbclient::K_N, ['b', 'B']),
        (orbclient::K_M, ['m', 'M']),
        (orbclient::K_COMMA, ['w', 'W']),
        (orbclient::K_PERIOD, ['v', 'V']),
        (orbclient::K_SLASH, ['z', 'Z']),
        (orbclient::K_SPACE, [' ', ' ']),
    ];

    pub static AZERTY: [(u8, [char; 2]); 53] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['&', '1']),
        (orbclient::K_2, ['é', '2']),
        (orbclient::K_3, ['"', '3']),
        (orbclient::K_4, ['\'', '4']),
        (orbclient::K_5, ['(', '5']),
        (orbclient::K_6, ['|', '6']),
        (orbclient::K_7, ['è', '7']),
        (orbclient::K_8, ['_', '8']),
        (orbclient::K_9, ['ç', '9']),
        (orbclient::K_0, ['à', '0']),
        (orbclient::K_MINUS, [')', '°']),
        (orbclient::K_EQUALS, ['=', '+']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['a', 'A']),
        (orbclient::K_W, ['z', 'Z']),
        (orbclient::K_E, ['e', 'E']),
        (orbclient::K_R, ['r', 'R']),
        (orbclient::K_T, ['t', 'T']),
        (orbclient::K_Y, ['y', 'Y']),
        (orbclient::K_U, ['u', 'U']),
        (orbclient::K_I, ['i', 'I']),
        (orbclient::K_O, ['o', 'O']),
        (orbclient::K_P, ['p', 'P']),
        (orbclient::K_BRACE_OPEN, ['^', '¨']),
        (orbclient::K_BRACE_CLOSE, ['$', '£']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\0', '\0']),
        (orbclient::K_A, ['q', 'Q']),
        (orbclient::K_S, ['s', 'S']),
        (orbclient::K_D, ['d', 'D']),
        (orbclient::K_F, ['f', 'F']),
        (orbclient::K_G, ['g', 'G']),
        (orbclient::K_H, ['h', 'H']),
        (orbclient::K_J, ['j', 'J']),
        (orbclient::K_K, ['k', 'K']),
        (orbclient::K_L, ['l', 'L']),
        (orbclient::K_SEMICOLON, ['m', 'M']),
        (orbclient::K_QUOTE, ['ù', '%']),
        (orbclient::K_TICK, ['*', 'µ']),
        (orbclient::K_BACKSLASH, ['ê', 'Ê']),
        (orbclient::K_Z, ['w', 'W']),
        (orbclient::K_X, ['x', 'X']),
        (orbclient::K_C, ['c', 'C']),
        (orbclient::K_V, ['v', 'V']),
        (orbclient::K_B, ['b', 'B']),
        (orbclient::K_N, ['n', 'N']),
        (orbclient::K_M, [',', '?']),
        (orbclient::K_COMMA, [';', '.']),
        (orbclient::K_PERIOD, [':', '/']),
        (orbclient::K_SLASH, ['!', '§']),
        (orbclient::K_SPACE, [' ', ' ']),
    ];

    pub static BEPO: [(u8, [char; 2]); 53] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['"', '1']),
        (orbclient::K_2, ['«', '2']),
        (orbclient::K_3, ['»', '3']),
        (orbclient::K_4, ['(', '4']),
        (orbclient::K_5, [')', '5']),
        (orbclient::K_6, ['@', '6']),
        (orbclient::K_7, ['+', '7']),
        (orbclient::K_8, ['-', '8']),
        (orbclient::K_9, ['/', '9']),
        (orbclient::K_0, ['*', '0']),
        (orbclient::K_MINUS, ['=', '°']),
        (orbclient::K_EQUALS, ['%', '`']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['b', 'B']),
        (orbclient::K_W, ['é', 'É']),
        (orbclient::K_E, ['p', 'P']),
        (orbclient::K_R, ['o', 'O']),
        (orbclient::K_T, ['è', 'È']),
        (orbclient::K_Y, ['^', '!']),
        (orbclient::K_U, ['v', 'V']),
        (orbclient::K_I, ['d', 'D']),
        (orbclient::K_O, ['l', 'L']),
        (orbclient::K_P, ['j', 'J']),
        (orbclient::K_BRACE_OPEN, ['z', 'Z']),
        (orbclient::K_BRACE_CLOSE, ['w', 'W']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\0', '\0']),
        (orbclient::K_A, ['a', 'A']),
        (orbclient::K_S, ['u', 'U']),
        (orbclient::K_D, ['i', 'I']),
        (orbclient::K_F, ['e', 'E']),
        (orbclient::K_G, [',', ';']),
        (orbclient::K_H, ['c', 'C']),
        (orbclient::K_J, ['t', 'T']),
        (orbclient::K_K, ['s', 'S']),
        (orbclient::K_L, ['r', 'R']),
        (orbclient::K_SEMICOLON, ['n', 'N']),
        (orbclient::K_QUOTE, ['m', 'M']),
        (orbclient::K_TICK, ['ç', 'Ç']),
        (orbclient::K_BACKSLASH, ['ê', 'Ê']),
        (orbclient::K_Z, ['à', 'À']),
        (orbclient::K_X, ['y', 'Y']),
        (orbclient::K_C, ['x', 'X']),
        (orbclient::K_V, ['.', ':']),
        (orbclient::K_B, ['k', 'K']),
        (orbclient::K_N, ['\'', '?']),
        (orbclient::K_M, ['q', 'Q']),
        (orbclient::K_COMMA, ['g', 'G']),
        (orbclient::K_PERIOD, ['h', 'H']),
        (orbclient::K_SLASH, ['f', 'F']),
        (orbclient::K_SPACE, [' ', ' ']),
    ];

    pub static IT: [(u8, [char; 2]); 53] = [
        (orbclient::K_ESC, ['\x1B', '\x1B']),
        (orbclient::K_1, ['1', '!']),
        (orbclient::K_2, ['2', '"']),
        (orbclient::K_3, ['3', '£']),
        (orbclient::K_4, ['4', '$']),
        (orbclient::K_5, ['5', '%']),
        (orbclient::K_6, ['6', '&']),
        (orbclient::K_7, ['7', '/']),
        (orbclient::K_8, ['8', '(']),
        (orbclient::K_9, ['9', ')']),
        (orbclient::K_0, ['0', '=']),
        (orbclient::K_MINUS, ['?', '\'']),
        (orbclient::K_EQUALS, ['ì', '^']),
        (orbclient::K_BKSP, ['\x7F', '\x7F']),
        (orbclient::K_TAB, ['\t', '\t']),
        (orbclient::K_Q, ['q', 'Q']),
        (orbclient::K_W, ['w', 'W']),
        (orbclient::K_E, ['e', 'E']),
        (orbclient::K_R, ['r', 'R']),
        (orbclient::K_T, ['t', 'T']),
        (orbclient::K_Y, ['y', 'Y']),
        (orbclient::K_U, ['u', 'U']),
        (orbclient::K_I, ['i', 'I']),
        (orbclient::K_O, ['o', 'O']),
        (orbclient::K_P, ['p', 'P']),
        (orbclient::K_BRACE_OPEN, ['è', 'é']),
        (orbclient::K_BRACE_CLOSE, ['+', '*']),
        (orbclient::K_ENTER, ['\n', '\n']),
        (orbclient::K_CTRL, ['\x20', '\x20']),
        (orbclient::K_A, ['a', 'A']),
        (orbclient::K_S, ['s', 'S']),
        (orbclient::K_D, ['d', 'D']),
        (orbclient::K_F, ['f', 'F']),
        (orbclient::K_G, ['g', 'G']),
        (orbclient::K_H, ['h', 'H']),
        (orbclient::K_J, ['j', 'J']),
        (orbclient::K_K, ['k', 'K']),
        (orbclient::K_L, ['l', 'L']),
        (orbclient::K_SEMICOLON, ['ò', 'ç']),
        (orbclient::K_QUOTE, ['à', '°']),
        (orbclient::K_TICK, ['ù', '§']),
        (orbclient::K_BACKSLASH, ['<', '>']),
        (orbclient::K_Z, ['z', 'Z']),
        (orbclient::K_X, ['x', 'X']),
        (orbclient::K_C, ['c', 'C']),
        (orbclient::K_V, ['v', 'V']),
        (orbclient::K_B, ['b', 'B']),
        (orbclient::K_N, ['n', 'N']),
        (orbclient::K_M, ['m', 'M']),
        (orbclient::K_COMMA, [',', ';']),
        (orbclient::K_PERIOD, ['.', ':']),
        (orbclient::K_SLASH, ['-', '_']),
        (orbclient::K_SPACE, [' ', ' ']),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(usize)]
pub enum KeymapKind {
    US = 0,
    GB,
    Dvorak,
    Azerty,
    Bepo,
    IT,
}

impl From<usize> for KeymapKind {
    fn from(value: usize) -> Self {
        if value > (KeymapKind::IT as usize) {
            KeymapKind::US
        } else {
            // SAFETY: Checked above
            unsafe { std::mem::transmute(value) }
        }
    }
}

#[allow(missing_copy_implementations)]
#[derive(Debug, PartialEq, Eq)]
pub struct ParseKeymapError(());

impl FromStr for KeymapKind {
    type Err = ParseKeymapError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let keymap = match s {
            "dvorak" => KeymapKind::Dvorak,
            "us" => KeymapKind::US,
            "gb" => KeymapKind::GB,
            "azerty" => KeymapKind::Azerty,
            "bepo" => KeymapKind::Bepo,
            "it" => KeymapKind::IT,
            &_ => return Err(ParseKeymapError(())),
        };

        Ok(keymap)
    }
}

impl Display for KeymapKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match *self {
            KeymapKind::US => "us",
            KeymapKind::GB => "gb",
            KeymapKind::Dvorak => "dvorak",
            KeymapKind::Azerty => "azerty",
            KeymapKind::Bepo => "bepo",
            KeymapKind::IT => "it",
        };
        f.write_str(s)
    }
}

pub struct KeymapData {
    pub keymap_hash: HashMap<u8, [char; 2]>,
    pub kind: KeymapKind,
}

impl KeymapData {
    pub fn new(kind: KeymapKind) -> Self {
        let keymap_hash = match kind {
            KeymapKind::US => HashMap::from(keymaps::US),
            KeymapKind::GB => HashMap::from(keymaps::GB),
            KeymapKind::Dvorak => HashMap::from(keymaps::DVORAK),
            KeymapKind::Azerty => HashMap::from(keymaps::AZERTY),
            KeymapKind::Bepo => HashMap::from(keymaps::BEPO),
            KeymapKind::IT => HashMap::from(keymaps::IT),
        };

        Self { keymap_hash, kind }
    }

    pub fn get_kind(&self) -> KeymapKind {
        self.kind
    }

    // TODO: AltGr, Numlock
    pub fn get_char(&self, scancode: u8, shift: bool) -> char {
        if let Some(c) = self.keymap_hash.get(&scancode) {
            if shift {
                c[1]
            } else {
                c[0]
            }
        } else {
            '\0'
        }
    }
}
