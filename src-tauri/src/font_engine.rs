use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use ttf_parser::{name_id, Face};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FontStandard {
    Unicode,
    RamdhenuGeetanjali,
    BijoySutonny,
    AutoMappedCustom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedFontProfile {
    pub id: String,
    pub family_name: String,
    pub standard: FontStandard,
    pub badge: String,
    pub custom_glyph_map: HashMap<String, String>,
}

pub fn scan_installed_assamese_fonts() -> Vec<DetectedFontProfile> {
    let mut profiles = vec![
        DetectedFontProfile {
            id: "builtin_ramdhenu".into(),
            family_name: "Geetanjali / Ramdhenu".into(),
            standard: FontStandard::RamdhenuGeetanjali,
            badge: "DTP".into(),
            custom_glyph_map: HashMap::new(),
        },
        DetectedFontProfile {
            id: "auto_unicode".into(),
            family_name: "Auto (Unicode)".into(),
            standard: FontStandard::Unicode,
            badge: "UNI".into(),
            custom_glyph_map: HashMap::new(),
        },
        DetectedFontProfile {
            id: "builtin_bijoy".into(),
            family_name: "SutonnyMJ / Bijoy".into(),
            standard: FontStandard::BijoySutonny,
            badge: "BJY".into(),
            custom_glyph_map: HashMap::new(),
        },
    ];

    let mut font_dirs: Vec<PathBuf> = vec![PathBuf::from(r"C:\Windows\Fonts")];
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        font_dirs.push(Path::new(&local_app).join(r"Microsoft\Windows\Fonts"));
    }

    let mut seen_names = std::collections::HashSet::new();

    for dir in font_dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if ext != "ttf" && ext != "otf" {
                continue;
            }

            if let Some(profile) = inspect_font_binary(&path) {
                let key = profile.family_name.to_ascii_lowercase();
                if seen_names.insert(key) {
                    profiles.push(profile);
                }
            }
        }
    }

    profiles
}

fn inspect_font_binary(path: &Path) -> Option<DetectedFontProfile> {
    let data = fs::read(path).ok()?;
    let face = Face::parse(&data, 0).ok()?;

    let family_name = face
        .names()
        .into_iter()
        .find(|n| n.name_id == name_id::FAMILY && n.is_unicode())
        .and_then(|n| n.to_string())
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })?;

    let lower = family_name.to_ascii_lowercase();

    let has_ka = face.glyph_index('ক').is_some();
    let has_ra_assam = face.glyph_index('ৰ').is_some();
    let has_ba = face.glyph_index('ব').is_some();

    if has_ka && (has_ra_assam || has_ba) {
        let is_indic_named = [
            "assam",
            "asom",
            "kalpurush",
            "mukti",
            "solaiman",
            "nikosh",
            "vrinda",
            "sonar",
            "rupali",
            "siyam",
            "ekushey",
            "noto sans bengali",
            "kohinoor",
            "tiro bangla",
            "baloo da",
            "hind siliguri",
            "anek bangla",
            "mina",
        ]
        .iter()
        .any(|kw| lower.contains(kw));

        if is_indic_named {
            return Some(DetectedFontProfile {
                id: format!("font_{}", lower.replace(' ', "_")),
                family_name,
                standard: FontStandard::Unicode,
                badge: "UNI".into(),
                custom_glyph_map: HashMap::new(),
            });
        }
        return None;
    }

    let is_ramdhenu_family = [
        "geetanjali",
        "asomiya",
        "borluit",
        "ramdhenu",
        "brahmaputra",
        "pragjyotish",
        "luit",
        "lakhimi",
        "surma",
        "barak",
        "axom",
        "assam",
    ]
    .iter()
    .any(|kw| lower.contains(kw));

    if is_ramdhenu_family {
        return Some(DetectedFontProfile {
            id: format!("font_{}", lower.replace(' ', "_")),
            family_name,
            standard: FontStandard::RamdhenuGeetanjali,
            badge: "DTP".into(),
            custom_glyph_map: HashMap::new(),
        });
    }

    let is_bijoy_family = [
        "sutonny",
        "bijoy",
        "sulekha",
        "tonny",
        "boishakhi",
        "falgun",
    ]
    .iter()
    .any(|kw| lower.contains(kw));

    if is_bijoy_family {
        return Some(DetectedFontProfile {
            id: format!("font_{}", lower.replace(' ', "_")),
            family_name,
            standard: FontStandard::BijoySutonny,
            badge: "BJY".into(),
            custom_glyph_map: HashMap::new(),
        });
    }

    None
}

pub fn shape_unicode_for_profile(unicode_text: &str, profile: &DetectedFontProfile) -> String {
    match profile.standard {
        FontStandard::Unicode => unicode_text.to_string(),
        FontStandard::RamdhenuGeetanjali => shape_ramdhenu_geetanjali(unicode_text),
        FontStandard::BijoySutonny => shape_bijoy_sutonny(unicode_text),
        FontStandard::AutoMappedCustom => unicode_text.to_string(),
    }
}

pub fn translate_ramdhenu_phonetic_fallback(ch: char) -> Option<(&'static str, bool)> {
    match ch {
        '`' => Some(("ং", false)),
        '1' => Some(("১", false)),
        '2' => Some(("২", false)),
        '3' => Some(("৩", false)),
        '4' => Some(("৪", false)),
        '5' => Some(("৫", false)),
        '6' => Some(("৬", false)),
        '7' => Some(("৭", false)),
        '8' => Some(("৮", false)),
        '9' => Some(("৯", false)),
        '0' => Some(("০", false)),
        '-' => Some(("-", false)),
        '=' => Some(("ৃ", false)),

        'q' => Some(("ৌ", false)),
        'w' => Some(("ৈ", false)),
        'e' => Some(("া", false)),
        'r' => Some(("ী", false)),
        't' => Some(("ূ", false)),
        'y' => Some(("ব", false)),
        'u' => Some(("হ", false)),
        'i' => Some(("গ", false)),
        'o' => Some(("দ", false)),
        'p' => Some(("জ", false)),
        '[' => Some(("ড", false)),
        ']' => Some(("়", false)),
        '\\' => Some(("য", false)),

        'a' => Some(("ো", false)),
        's' => Some(("ে", false)),
        'd' => Some(("্", false)),
        'f' => Some(("ি", false)),
        'g' => Some(("ু", false)),
        'h' => Some(("প", false)),
        'j' => Some(("ৰ", false)),
        'k' => Some(("ক", false)),
        'l' => Some(("ত", false)),
        ';' => Some(("চ", false)),
        '\'' => Some(("ট", false)),

        'z' => Some(("্ৰ", false)),
        'x' => Some(("ং", false)),
        'c' => Some(("ম", false)),
        'v' => Some(("ন", false)),
        'b' => Some(("ৱ", false)),
        'n' => Some(("ল", false)),
        'm' => Some(("স", false)),
        ',' => Some((",", false)),
        '.' => Some((".", false)),
        '/' => Some(("য়", false)),

        '~' => Some(("‘", false)),
        '!' => Some(("হ্ম", false)),
        '@' => Some(("’", false)),
        '#' => Some((":", false)),
        '$' => Some(("/", false)),
        '%' => Some((";", false)),
        '^' => Some(("ত্ৰ", false)),
        '&' => Some(("ক্ষ", false)),
        '*' => Some(("জ্ঞ", false)),
        '(' => Some(("(", false)),
        ')' => Some((")", false)),
        '_' => Some(("ঃ", false)),
        '+' => Some(("ঋ", false)),

        'Q' => Some(("ঔ", false)),
        'W' => Some(("ঐ", false)),
        'E' => Some(("আ", false)),
        'R' => Some(("ঈ", false)),
        'T' => Some(("ঊ", false)),
        'Y' => Some(("ভ", false)),
        'U' => Some(("ঙ", false)),
        'I' => Some(("ঘ", false)),
        'O' => Some(("ধ", false)),
        'P' => Some(("ঝ", false)),
        '{' => Some(("ঢ", false)),
        '}' => Some(("ঞ", false)),
        '|' => Some(("ৎ", false)),

        'A' => Some(("ও", false)),
        'S' => Some(("এ", false)),
        'D' => Some(("অ", false)),
        'F' => Some(("ই", false)),
        'G' => Some(("উ", false)),
        'H' => Some(("ফ", false)),
        'J' => Some(("ৰ", false)),
        'K' => Some(("খ", false)),
        'L' => Some(("থ", false)),
        ':' => Some(("ছ", false)),
        '"' => Some(("ঠ", false)),

        'Z' => Some(("ৰ্", true)),
        'X' => Some(("ঁ", false)),
        'C' => Some(("ণ", false)),
        'V' => Some(("V", false)),
        'B' => Some(("B", false)),
        'N' => Some(("ঙ্গ", false)),
        'M' => Some(("শ", false)),
        '<' => Some(("ষ", false)),
        '>' => Some(("।", false)),
        '?' => Some(("?", false)),

        _ => None,
    }
}

fn is_consonant(ch: char) -> bool {
    matches!(ch, 'ক'..='হ' | 'ড়' | 'ঢ়' | 'য়' | 'ৰ' | 'ৱ')
}

fn is_matra(ch: char) -> bool {
    matches!(ch, 'া' | 'ি' | 'ী' | 'ু' | 'ূ' | 'ৃ' | 'ে' | 'ৈ' | 'ো' | 'ৌ')
}

fn shape_ramdhenu_geetanjali(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        let mut has_reph = false;
        if i + 2 < chars.len()
            && chars[i] == 'ৰ'
            && chars[i + 1] == '্'
            && is_consonant(chars[i + 2])
        {
            has_reph = true;
            i += 2;
        }

        if is_consonant(chars[i]) {
            let mut cluster = String::new();
            cluster.push(chars[i]);
            i += 1;

            while i + 1 < chars.len() && chars[i] == '্' && is_consonant(chars[i + 1]) {
                cluster.push('্');
                cluster.push(chars[i + 1]);
                i += 2;
            }

            let trailing_hasanta = if i < chars.len() && chars[i] == '্' {
                i += 1;
                true
            } else {
                false
            };

            let matra = if i < chars.len() && is_matra(chars[i]) {
                let m = chars[i];
                i += 1;
                Some(m)
            } else {
                None
            };

            let base_glyph = map_ramdhenu_cluster(&cluster);
            let reph_str = if has_reph { "Z" } else { "" };

            match matra {
                Some('ি') => {
                    out.push('f');
                    out.push_str(&base_glyph);
                    out.push_str(reph_str);
                }
                Some('ে') => {
                    out.push('s');
                    out.push_str(&base_glyph);
                    out.push_str(reph_str);
                }
                Some('ৈ') => {
                    out.push('w');
                    out.push_str(&base_glyph);
                    out.push_str(reph_str);
                }
                Some('ো') => {
                    out.push('a');
                    out.push_str(&base_glyph);
                    out.push_str(reph_str);
                }
                Some('ৌ') => {
                    out.push('q');
                    out.push_str(&base_glyph);
                    out.push_str(reph_str);
                }
                Some('া') => {
                    out.push_str(&base_glyph);
                    out.push('e');
                    out.push_str(reph_str);
                }
                Some('ী') => {
                    out.push_str(&base_glyph);
                    out.push('r');
                    out.push_str(reph_str);
                }
                Some('ু') => {
                    out.push_str(&base_glyph);
                    out.push('g');
                    out.push_str(reph_str);
                }
                Some('ূ') => {
                    out.push_str(&base_glyph);
                    out.push('t');
                    out.push_str(reph_str);
                }
                Some('ৃ') => {
                    out.push_str(&base_glyph);
                    out.push('=');
                    out.push_str(reph_str);
                }
                _ => {
                    out.push_str(&base_glyph);
                    if trailing_hasanta {
                        out.push('d');
                    }
                    out.push_str(reph_str);
                }
            }
        } else {
            out.push_str(map_ramdhenu_single(chars[i]));
            i += 1;
        }
    }

    out
}

fn map_ramdhenu_cluster(cluster: &str) -> String {
    let lig = match cluster {
        "ক্ষ" => Some("&"),
        "জ্ঞ" => Some("*"),
        "ত্ৰ" => Some("^"),
        "হ্ম" => Some("!"),
        "ঙ্গ" => Some("N"),
        "শ্ৰ" => Some("Mz"),
        "প্ৰ" => Some("hz"),
        "ক্ৰ" => Some("kz"),
        _ => None,
    };

    if let Some(mapped) = lig {
        return mapped.to_string();
    }

    if let Some(base) = cluster.strip_suffix("্ৰ") {
        return format!("{}z", map_ramdhenu_cluster(base));
    }

    let parts: Vec<&str> = cluster.split('্').collect();
    if parts.len() == 1 {
        if let Some(ch) = parts[0].chars().next() {
            return map_ramdhenu_single(ch).to_string();
        }
    }

    parts
        .iter()
        .filter_map(|p| p.chars().next())
        .map(map_ramdhenu_single)
        .collect::<Vec<_>>()
        .join("d")
}

fn map_ramdhenu_single(ch: char) -> &'static str {
    match ch {
        'অ' => "D",
        'আ' => "E",
        'ই' => "F",
        'ঈ' => "R",
        'উ' => "H",
        'ঊ' => "T",
        'ঋ' => "+",
        'এ' => "S",
        'ঐ' => "W",
        'ও' => "A",
        'ঔ' => "Q",

        'ক' => "k",
        'খ' => "K",
        'গ' => "i",
        'ঘ' => "I",
        'ঙ' => "U",
        'চ' => ";",
        'ছ' => ":",
        'জ' => "p",
        'ঝ' => "P",
        'ঞ' => "}",
        'ট' => "'",
        'ঠ' => "\"",
        'ড' => "[",
        'ঢ' => "{",
        'ণ' => "C",
        'ত' => "l",
        'থ' => "L",
        'দ' => "o",
        'ধ' => "O",
        'ন' => "v",
        'প' => "h",
        'ফ' => "H",
        'ব' => "y",
        'ভ' => "Y",
        'ম' => "c",
        'য' => "\\",
        'ৰ' => "j",
        'ল' => "n",
        'ৱ' => "b",
        'শ' => "M",
        'ষ' => "<",
        'স' => "m",
        'হ' => "u",
        'ড়' => "[",
        'ঢ়' => "{",
        'য়' => "/",
        'ৎ' => "|",

        'ং' => "x",
        'ঃ' => "_",
        'ঁ' => "X",
        '।' => ">",
        '০' => "0",
        '১' => "1",
        '২' => "2",
        '৩' => "3",
        '৪' => "4",
        '৫' => "5",
        '৬' => "6",
        '৭' => "7",
        '৮' => "8",
        '৯' => "9",
        '-' => "-",
        ',' => ",",
        '.' => ".",
        '(' => "(",
        ')' => ")",
        '?' => "?",
        _ => "",
    }
}

fn shape_bijoy_sutonny(input: &str) -> String {
    input.to_string()
}
