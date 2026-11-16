use read_fonts::{FontRef, TableProvider};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use ttf_parser::{name_id, Face};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::AddFontResourceExW;
use windows::Win32::UI::WindowsAndMessaging::{SendNotifyMessageW, HWND_BROADCAST, WM_FONTCHANGE};
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::Os2;
use write_fonts::types::{GlyphId as FontGlyphId, NameId};
use write_fonts::FontBuilder;

const CONVERTER_SCHEMA_VERSION: &str = "v2_os2_unicode_range";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum LegacyScriptFamily {
    RamdhenuAssamese, // Geetanjali, Asomiya, Borluit, Ramdhenu
    BijoyBengali,     // SutonnyMJ, Bijoy, Sulekha
    KrutiDevHindi,    // Kruti Dev, DevLys, Chanakya
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConvertedFontInfo {
    pub id: String,
    pub original_name: String,
    pub unicode_family_name: String,
    pub script_label: String,
    pub output_path: String,
    pub newly_converted: bool,
}

pub fn auto_convert_and_register_fonts() -> Vec<ConvertedFontInfo> {
    let mut results = vec![ConvertedFontInfo {
        id: "auto_unicode".into(),
        original_name: "System Unicode".into(),
        unicode_family_name: "Auto (Universal Unicode)".into(),
        script_label: "UNI".into(),
        output_path: String::new(),
        newly_converted: false,
    }];

    let out_dir = get_converted_fonts_dir();
    let _ = fs::create_dir_all(&out_dir);

    // Automatically rebuild cached converted fonts when the table schema upgrades
    let version_marker = out_dir.join(".schema_version");
    let needs_rebuild = fs::read_to_string(&version_marker)
        .map(|v| v.trim() != CONVERTER_SCHEMA_VERSION)
        .unwrap_or(true);

    if needs_rebuild {
        if let Ok(existing) = fs::read_dir(&out_dir) {
            for entry in existing.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("ttf") {
                    let _ = fs::remove_file(p);
                }
            }
        }
        let _ = fs::write(&version_marker, CONVERTER_SCHEMA_VERSION);
    }

    let mut scan_dirs: Vec<PathBuf> = vec![PathBuf::from(r"C:\Windows\Fonts")];
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        scan_dirs.push(Path::new(&local_app).join(r"Microsoft\Windows\Fonts"));
    }

    let mut seen_families = std::collections::HashSet::new();
    let mut installed_any_new = false;

    for dir in scan_dirs {
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

            if ext != "ttf" {
                continue;
            }

            let Some(analysis) = classify_font_file(&path) else {
                continue;
            };

            match analysis {
                FontClassification::AlreadyUnicode {
                    family_name,
                    script,
                } => {
                    let key = family_name.to_ascii_lowercase();
                    if seen_families.insert(key.clone()) {
                        results.push(ConvertedFontInfo {
                            id: format!("uni_{}", key.replace(' ', "_")),
                            original_name: family_name.clone(),
                            unicode_family_name: family_name,
                            script_label: script,
                            output_path: path.to_string_lossy().to_string(),
                            newly_converted: false,
                        });
                    }
                }
                FontClassification::LegacyNeedsConversion {
                    family_name,
                    legacy_type,
                } => {
                    let unicode_name = format!("{} Unicode", family_name);
                    let key = unicode_name.to_ascii_lowercase();
                    if !seen_families.insert(key.clone()) {
                        continue;
                    }

                    let safe_filename = format!(
                        "{}_Unicode.ttf",
                        family_name
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                            .collect::<String>()
                    );
                    let dest_path = out_dir.join(&safe_filename);

                    let mut newly_converted = false;
                    if !dest_path.exists() {
                        if convert_legacy_ttf_to_unicode(
                            &path,
                            &dest_path,
                            &unicode_name,
                            &legacy_type,
                        )
                        .is_ok()
                        {
                            register_font_for_current_user(&dest_path, &unicode_name);
                            newly_converted = true;
                            installed_any_new = true;
                        }
                    } else {
                        register_font_session_only(&dest_path);
                    }

                    if dest_path.exists() {
                        let badge = match legacy_type {
                            LegacyScriptFamily::RamdhenuAssamese => "ASM-UNI",
                            LegacyScriptFamily::BijoyBengali => "BNG-UNI",
                            LegacyScriptFamily::KrutiDevHindi => "HIN-UNI",
                        };

                        results.push(ConvertedFontInfo {
                            id: format!("conv_{}", key.replace(' ', "_")),
                            original_name: family_name,
                            unicode_family_name: unicode_name,
                            script_label: badge.into(),
                            output_path: dest_path.to_string_lossy().to_string(),
                            newly_converted,
                        });
                    }
                }
            }
        }
    }

    if installed_any_new {
        unsafe {
            let _ = SendNotifyMessageW(HWND_BROADCAST, WM_FONTCHANGE, WPARAM(0), LPARAM(0));
        }
    }

    results
}

fn get_converted_fonts_dir() -> PathBuf {
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        Path::new(&local_app)
            .join("PranabKeyboard")
            .join("ConvertedFonts")
    } else {
        PathBuf::from(r"C:\Users\Public\PranabKeyboard\ConvertedFonts")
    }
}

enum FontClassification {
    AlreadyUnicode {
        family_name: String,
        script: String,
    },
    LegacyNeedsConversion {
        family_name: String,
        legacy_type: LegacyScriptFamily,
    },
}

fn classify_font_file(path: &Path) -> Option<FontClassification> {
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

    if lower.ends_with(" unicode") {
        return None;
    }

    let has_bengali_ka = face.glyph_index('ক').is_some();
    let has_hindi_ka = face.glyph_index('क').is_some();

    if has_bengali_ka || has_hindi_ka {
        let is_indic_font = [
            "assam",
            "asom",
            "geetanjali",
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
            "noto serif bengali",
            "noto sans devanagari",
            "mangal",
            "aparajita",
            "kokila",
            "utsaah",
            "nirmala",
            "kohinoor",
            "tiro",
            "hind",
            "baloo",
        ]
        .iter()
        .any(|kw| lower.contains(kw));

        if is_indic_font {
            let script = if has_bengali_ka { "ASM/BNG" } else { "HINDI" };
            return Some(FontClassification::AlreadyUnicode {
                family_name,
                script: script.into(),
            });
        }
        return None;
    }

    if [
        "geetanjali",
        "asomiya",
        "borluit",
        "ramdhenu",
        "brahmaputra",
        "pragjyotish",
        "luit",
        "lakhimi",
        "axom",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            family_name,
            legacy_type: LegacyScriptFamily::RamdhenuAssamese,
        });
    }

    if [
        "sutonny",
        "bijoy",
        "sulekha",
        "tonny",
        "boishakhi",
        "falgun",
        "padma",
        "meghna",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            family_name,
            legacy_type: LegacyScriptFamily::BijoyBengali,
        });
    }

    if ["kruti", "devlys", "chanakya", "agra", "kundli", "shusha"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            family_name,
            legacy_type: LegacyScriptFamily::KrutiDevHindi,
        });
    }

    None
}

fn convert_legacy_ttf_to_unicode(
    src_path: &Path,
    dest_path: &Path,
    new_family_name: &str,
    legacy_type: &LegacyScriptFamily,
) -> Result<(), String> {
    let raw_bytes = fs::read(src_path).map_err(|e| e.to_string())?;
    let parsed_face = Face::parse(&raw_bytes, 0).map_err(|e| e.to_string())?;
    let font_ref = FontRef::new(&raw_bytes).map_err(|e| e.to_string())?;

    let legacy_pairs = get_legacy_to_unicode_table(legacy_type);
    let mut unicode_to_gid: BTreeMap<u32, u16> = BTreeMap::new();

    for ascii_ch in [' ', '.', ',', '-', '(', ')', '?', '!', ':', ';', '/'] {
        if let Some(gid) = parsed_face.glyph_index(ascii_ch) {
            unicode_to_gid.insert(ascii_ch as u32, gid.0);
        }
    }

    for (legacy_ch, unicode_ch) in legacy_pairs {
        if let Some(gid) = parsed_face.glyph_index(legacy_ch) {
            if gid.0 != 0 {
                unicode_to_gid.insert(unicode_ch as u32, gid.0);
            }
        }
    }

    if let Some(&ra_gid) = unicode_to_gid.get(&('ৰ' as u32)) {
        unicode_to_gid.entry('র' as u32).or_insert(ra_gid);
    }

    let cmap = build_unicode_cmap(&unicode_to_gid)?;
    let name_table = build_unicode_name_table(new_family_name);

    let mut builder = FontBuilder::new();
    builder.add_table(&cmap).map_err(|e| e.to_string())?;
    builder.add_table(&name_table).map_err(|e| e.to_string())?;

    // Patch OS/2 Unicode Range bits so MS Word / DirectWrite locks onto this font instead of falling back to Shonar Bangla
    if let Ok(os2_ref) = font_ref.os2() {
        let mut os2_owned: Os2 = os2_ref.to_owned_table();
        match legacy_type {
            LegacyScriptFamily::RamdhenuAssamese | LegacyScriptFamily::BijoyBengali => {
                os2_owned.ul_unicode_range_1 |= 1 << 16; // Bit 16 = Bengali & Assamese (U+0980..U+09FF)
            }
            LegacyScriptFamily::KrutiDevHindi => {
                os2_owned.ul_unicode_range_1 |= 1 << 15; // Bit 15 = Devanagari (U+0900..U+097F)
            }
        }
        builder.add_table(&os2_owned).map_err(|e| e.to_string())?;
    }

    builder.copy_missing_tables(font_ref);

    let built_bytes = builder.build();
    fs::write(dest_path, built_bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn get_legacy_to_unicode_table(legacy_type: &LegacyScriptFamily) -> Vec<(char, char)> {
    match legacy_type {
        LegacyScriptFamily::RamdhenuAssamese => vec![
            ('D', 'অ'),
            ('E', 'আ'),
            ('F', 'ই'),
            ('R', 'ঈ'),
            ('H', 'উ'),
            ('T', 'ঊ'),
            ('+', 'ঋ'),
            ('S', 'এ'),
            ('W', 'ঐ'),
            ('A', 'ও'),
            ('Q', 'ঔ'),
            ('k', 'ক'),
            ('K', 'খ'),
            ('i', 'গ'),
            ('I', 'ঘ'),
            ('U', 'ঙ'),
            (';', 'চ'),
            (':', 'ছ'),
            ('p', 'জ'),
            ('P', 'ঝ'),
            ('}', 'ঞ'),
            ('\'', 'ট'),
            ('"', 'ঠ'),
            ('[', 'ড'),
            ('{', 'ঢ'),
            ('C', 'ণ'),
            ('l', 'ত'),
            ('L', 'থ'),
            ('o', 'দ'),
            ('O', 'ধ'),
            ('v', 'ন'),
            ('h', 'প'),
            ('y', 'ব'),
            ('Y', 'ভ'),
            ('c', 'ম'),
            ('\\', 'য'),
            ('j', 'ৰ'),
            ('n', 'ল'),
            ('b', 'ৱ'),
            ('M', 'শ'),
            ('<', 'ষ'),
            ('m', 'স'),
            ('u', 'হ'),
            ('/', 'য়'),
            ('|', 'ৎ'),
            ('e', 'া'),
            ('f', 'ি'),
            ('r', 'ী'),
            ('g', 'ু'),
            ('t', 'ূ'),
            ('=', 'ৃ'),
            ('s', 'ে'),
            ('w', 'ৈ'),
            ('a', 'ো'),
            ('q', 'ৌ'),
            ('d', '্'),
            ('x', 'ং'),
            ('_', 'ঃ'),
            ('X', 'ঁ'),
            ('>', '।'),
            ('0', '০'),
            ('1', '১'),
            ('2', '২'),
            ('3', '৩'),
            ('4', '৪'),
            ('5', '৫'),
            ('6', '৬'),
            ('7', '৭'),
            ('8', '৮'),
            ('9', '৯'),
        ],
        LegacyScriptFamily::BijoyBengali => vec![
            ('A', 'অ'),
            ('B', 'ই'),
            ('C', 'ঈ'),
            ('D', 'উ'),
            ('E', 'ঊ'),
            ('F', 'ঋ'),
            ('G', 'এ'),
            ('H', 'ঐ'),
            ('I', 'ও'),
            ('J', 'ঔ'),
            ('K', 'ক'),
            ('L', 'খ'),
            ('M', 'গ'),
            ('N', 'ঘ'),
            ('O', 'ঙ'),
            ('P', 'চ'),
            ('Q', 'ছ'),
            ('R', 'জ'),
            ('S', 'ঝ'),
            ('T', 'ঞ'),
            ('U', 'ট'),
            ('V', 'ঠ'),
            ('W', 'ড'),
            ('X', 'ঢ'),
            ('Y', 'ণ'),
            ('Z', 'ত'),
            ('_', 'থ'),
            ('`', 'দ'),
            ('a', 'ধ'),
            ('b', 'ন'),
            ('c', 'প'),
            ('d', 'ফ'),
            ('e', 'ব'),
            ('f', 'ভ'),
            ('g', 'ম'),
            ('h', 'য'),
            ('i', 'ৰ'),
            ('j', 'ল'),
            ('k', 'শ'),
            ('l', 'ষ'),
            ('m', 'স'),
            ('n', 'হ'),
            ('o', 'ড়'),
            ('p', 'ঢ়'),
            ('q', 'য়'),
            ('r', 'ৎ'),
            ('s', 'ং'),
            ('t', 'ঃ'),
            ('u', 'ঁ'),
            ('v', 'া'),
            ('w', 'ি'),
            ('x', 'ী'),
            ('y', 'ু'),
            ('~', 'ূ'),
            ('&', '্'),
            ('|', '।'),
        ],
        LegacyScriptFamily::KrutiDevHindi => vec![
            ('v', 'अ'),
            ('b', 'इ'),
            ('m', 'उ'),
            (',', 'ए'),
            ('d', 'क'),
            ('[', 'ख'),
            ('x', 'ग'),
            ('?', 'घ'),
            ('p', 'च'),
            ('N', 'छ'),
            ('t', 'ज'),
            ('>', 'झ'),
            ('V', 'ट'),
            ('B', 'ठ'),
            ('M', 'ड'),
            ('<', 'ढ'),
            ('r', 'त'),
            ('F', 'थ'),
            ('n', 'द'),
            ('/', 'ध'),
            ('u', 'न'),
            ('i', 'प'),
            ('Q', 'फ'),
            ('c', 'ब'),
            ('H', 'भ'),
            ('e', 'म'),
            (';', 'य'),
            ('j', 'र'),
            ('y', 'ल'),
            ('o', 'व'),
            ('\'', 'श'),
            ('"', 'ष'),
            ('l', 'स'),
            ('g', 'ह'),
            ('k', 'ा'),
            ('f', 'ि'),
            ('h', 'ी'),
            ('q', 'ु'),
            ('w', 'ू'),
            ('s', 'े'),
            ('S', 'ै'),
            ('~', '्'),
            ('a', 'ं'),
            ('A', '।'),
        ],
    }
}

fn build_unicode_cmap(map: &BTreeMap<u32, u16>) -> Result<Cmap, String> {
    let mappings = map
        .iter()
        .map(|(&codepoint, &glyph_id)| {
            let character = char::from_u32(codepoint)
                .ok_or_else(|| format!("Invalid Unicode code point: {codepoint:#X}"))?;
            Ok((character, FontGlyphId::from(glyph_id)))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Cmap::from_mappings(mappings).map_err(|error| error.to_string())
}

fn build_unicode_name_table(family_name: &str) -> Name {
    let ps_name: String = family_name.chars().filter(|c| !c.is_whitespace()).collect();
    let records = vec![
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::FAMILY_NAME,
            family_name.to_owned().into(),
        ),
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::SUBFAMILY_NAME,
            String::from("Regular").into(),
        ),
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::FULL_NAME,
            family_name.to_owned().into(),
        ),
        NameRecord::new(3, 1, 0x0409, NameId::POSTSCRIPT_NAME, ps_name.into()),
    ];
    Name::new(records)
}

fn register_font_for_current_user(ttf_path: &Path, display_name: &str) {
    register_font_session_only(ttf_path);

    let reg_key = format!("{} (TrueType)", display_name);
    let path_str = ttf_path.to_string_lossy().to_string();

    let _ = std::process::Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
            "/v",
            &reg_key,
            "/t",
            "REG_SZ",
            "/d",
            &path_str,
            "/f",
        ])
        .output();
}

fn register_font_session_only(ttf_path: &Path) {
    let mut wide: Vec<u16> = ttf_path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = AddFontResourceExW(PCWSTR(wide.as_mut_ptr()), Default::default(), None);
    }
}
