use std::collections::HashMap;

use crate::keymap::{Keymap, KeymapEntry};

fn xkb_keycode_to_scancode(code: &str) -> Option<u8> {
    match code {
        "TLDE" => Some(0x29),
        "AE01" => Some(0x02),
        "AE02" => Some(0x03),
        "AE03" => Some(0x04),
        "AE04" => Some(0x05),
        "AE05" => Some(0x06),
        "AE06" => Some(0x07),
        "AE07" => Some(0x08),
        "AE08" => Some(0x09),
        "AE09" => Some(0x0A),
        "AE10" => Some(0x0B),
        "AE11" => Some(0x0C),
        "AE12" => Some(0x0D),
        "AD01" => Some(0x10),
        "AD02" => Some(0x11),
        "AD03" => Some(0x12),
        "AD04" => Some(0x13),
        "AD05" => Some(0x14),
        "AD06" => Some(0x15),
        "AD07" => Some(0x16),
        "AD08" => Some(0x17),
        "AD09" => Some(0x18),
        "AD10" => Some(0x19),
        "AD11" => Some(0x1A),
        "AD12" => Some(0x1B),
        "AC01" => Some(0x1E),
        "AC02" => Some(0x1F),
        "AC03" => Some(0x20),
        "AC04" => Some(0x21),
        "AC05" => Some(0x22),
        "AC06" => Some(0x23),
        "AC07" => Some(0x24),
        "AC08" => Some(0x25),
        "AC09" => Some(0x26),
        "AC10" => Some(0x27),
        "AC11" => Some(0x28),
        "BKSL" => Some(0x2B),
        "AB01" => Some(0x2C),
        "AB02" => Some(0x2D),
        "AB03" => Some(0x2E),
        "AB04" => Some(0x2F),
        "AB05" => Some(0x30),
        "AB06" => Some(0x31),
        "AB07" => Some(0x32),
        "AB08" => Some(0x33),
        "AB09" => Some(0x34),
        "AB10" => Some(0x35),
        "SPCE" => Some(0x39),
        "LSGT" => Some(0x56),
        "BKSP" => Some(0x0E),
        "TAB" => Some(0x0F),
        "RTRN" => Some(0x1C),
        _ => None,
    }
}

fn keysym_to_char(sym: &str) -> char {
    if sym.len() == 1 {
        return sym.chars().next().unwrap_or('\0');
    }
    match sym {
        "space" => ' ',
        "exclam" => '!',
        "quotedbl" => '"',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "ampersand" => '&',
        "apostrophe" => '\'',
        "quoteright" => '\'',
        "parenleft" => '(',
        "parenright" => ')',
        "asterisk" => '*',
        "plus" => '+',
        "comma" => ',',
        "minus" => '-',
        "period" => '.',
        "slash" => '/',
        "colon" => ':',
        "semicolon" => ';',
        "less" => '<',
        "equal" => '=',
        "greater" => '>',
        "question" => '?',
        "at" => '@',
        "bracketleft" => '[',
        "backslash" => '\\',
        "bracketright" => ']',
        "asciicircum" => '^',
        "underscore" => '_',
        "grave" => '`',
        "braceleft" => '{',
        "bar" => '|',
        "braceright" => '}',
        "asciitilde" => '~',
        "nobreakspace" => '\u{00A0}',
        "exclamdown" => '¡',
        "cent" => '¢',
        "sterling" => '£',
        "currency" => '¤',
        "yen" => '¥',
        "brokenbar" => '¦',
        "section" => '§',
        "diaeresis" => '¨',
        "copyright" => '©',
        "ordfeminine" => 'ª',
        "guillemotleft" => '«',
        "notsign" => '¬',
        "hyphen" => '\u{00AD}',
        "registered" => '®',
        "macron" => '¯',
        "degree" => '°',
        "plusminus" => '±',
        "twosuperior" => '²',
        "threesuperior" => '³',
        "acute" => '´',
        "mu" => 'µ',
        "paragraph" => '¶',
        "periodcentered" => '·',
        "cedilla" => '¸',
        "onesuperior" => '¹',
        "masculine" => 'º',
        "guillemotright" => '»',
        "onequarter" => '¼',
        "onehalf" => '½',
        "threequarters" => '¾',
        "questiondown" => '¿',
        "Agrave" => 'À',
        "Aacute" => 'Á',
        "Acircumflex" => 'Â',
        "Atilde" => 'Ã',
        "Adiaeresis" => 'Ä',
        "Aring" => 'Å',
        "AE" => 'Æ',
        "Ccedilla" => 'Ç',
        "Egrave" => 'È',
        "Eacute" => 'É',
        "Ecircumflex" => 'Ê',
        "Ediaeresis" => 'Ë',
        "Igrave" => 'Ì',
        "Iacute" => 'Í',
        "Icircumflex" => 'Î',
        "Idiaeresis" => 'Ï',
        "ETH" => 'Ð',
        "Ntilde" => 'Ñ',
        "Ograve" => 'Ò',
        "Oacute" => 'Ó',
        "Ocircumflex" => 'Ô',
        "Otilde" => 'Õ',
        "Odiaeresis" => 'Ö',
        "multiply" => '×',
        "Ooblique" => 'Ø',
        "Ugrave" => 'Ù',
        "Uacute" => 'Ú',
        "Ucircumflex" => 'Û',
        "Udiaeresis" => 'Ü',
        "Yacute" => 'Ý',
        "THORN" => 'Þ',
        "ssharp" => 'ß',
        "agrave" => 'à',
        "aacute" => 'á',
        "acircumflex" => 'â',
        "atilde" => 'ã',
        "adiaeresis" => 'ä',
        "aring" => 'å',
        "ae" => 'æ',
        "ccedilla" => 'ç',
        "egrave" => 'è',
        "eacute" => 'é',
        "ecircumflex" => 'ê',
        "ediaeresis" => 'ë',
        "igrave" => 'ì',
        "iacute" => 'í',
        "icircumflex" => 'î',
        "idiaeresis" => 'ï',
        "eth" => 'ð',
        "ntilde" => 'ñ',
        "ograve" => 'ò',
        "oacute" => 'ó',
        "ocircumflex" => 'ô',
        "otilde" => 'õ',
        "odiaeresis" => 'ö',
        "division" => '÷',
        "oslash" => 'ø',
        "ugrave" => 'ù',
        "uacute" => 'ú',
        "ucircumflex" => 'û',
        "udiaeresis" => 'ü',
        "yacute" => 'ý',
        "thorn" => 'þ',
        "ydiaeresis" => 'ÿ',
        "EuroSign" => '€',
        "NoSymbol" => '\0',
        _ => '\0',
    }
}

struct XkbKeyEntry {
    scancode: u8,
    normal: char,
    shifted: char,
    altgr: char,
    altgr_shifted: char,
}

fn parse_keysyms(syms: &[&str]) -> (char, char, char, char) {
    let normal = syms.get(0).map(|s| keysym_to_char(s.trim())).unwrap_or('\0');
    let shifted = syms.get(1).map(|s| keysym_to_char(s.trim())).unwrap_or('\0');
    let altgr = syms.get(2).map(|s| keysym_to_char(s.trim())).unwrap_or('\0');
    let altgr_shifted = syms.get(3).map(|s| keysym_to_char(s.trim())).unwrap_or('\0');
    (normal, shifted, altgr, altgr_shifted)
}

pub fn parse_xkb_symbols(content: &str, variant: Option<&str>) -> Result<Keymap, String> {
    let target_variant = variant.unwrap_or("basic");
    let mut entries: HashMap<u8, KeymapEntry> = HashMap::new();
    let mut found_variant = false;

    let mut i = 0;
    let lines: Vec<&str> = content.lines().collect();
    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("xkb_symbols") {
            let name = extract_variant_name(line).unwrap_or("basic");
            if name != target_variant {
                i = skip_brace_block(&lines, i + 1);
                continue;
            }
            found_variant = true;
            i += 1;
            while i < lines.len() {
                let inner = lines[i].trim();
                if inner.starts_with('}') {
                    break;
                }
                if inner.starts_with("key <") {
                    if let Some(entry) = parse_key_line(inner) {
                        entries.entry(entry.scancode).or_insert_with(|| {
                            KeymapEntry {
                                scancode: entry.scancode,
                                normal: entry.normal,
                                shifted: entry.shifted,
                                altgr: entry.altgr,
                                altgr_shifted: entry.altgr_shifted,
                            }
                        });
                    }
                }
                i += 1;
            }
        }
        i += 1;
    }

    if !found_variant {
        return Err(format!("variant '{}' not found in XKB symbols file", target_variant));
    }

    Ok(Keymap {
        name: variant.unwrap_or("basic").to_string(),
        entries,
        compose: Vec::new(),
        dead_keys: Vec::new(),
    })
}

fn extract_variant_name(line: &str) -> Option<&str> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn skip_brace_block(lines: &[&str], start: usize) -> usize {
    let mut depth = 1;
    let mut i = start;
    while i < lines.len() && depth > 0 {
        for ch in lines[i].chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    i
}

fn parse_key_line(line: &str) -> Option<XkbKeyEntry> {
    let key_start = line.find('<')?;
    let key_end = line[key_start + 1..].find('>')?;
    let keycode = &line[key_start + 1..key_start + 1 + key_end];
    let scancode = xkb_keycode_to_scancode(keycode)?;

    let syms_start = line.find('[')?;
    let syms_end = line.rfind(']')?;
    let syms_content = &line[syms_start + 1..syms_end];
    let syms: Vec<&str> = syms_content.split(',').collect();

    let (normal, shifted, altgr, altgr_shifted) = parse_keysyms(&syms);

    Some(XkbKeyEntry {
        scancode,
        normal,
        shifted,
        altgr,
        altgr_shifted,
    })
}

pub fn load_xkb_keymap(xkb_dir: &str, layout: &str, variant: Option<&str>) -> Result<Keymap, String> {
    let file_path = format!("{}/symbols/{}", xkb_dir, layout);
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read XKB symbols file '{}': {}", file_path, e))?;
    parse_xkb_symbols(&content, variant)
}
