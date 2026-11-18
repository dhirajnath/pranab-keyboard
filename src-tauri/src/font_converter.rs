use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use ttf_parser::{name_id, Face, GlyphId};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::AddFontResourceExW;
use windows::Win32::UI::WindowsAndMessaging::{SendNotifyMessageW, HWND_BROADCAST, WM_FONTCHANGE};
use write_fonts::from_obj::ToOwnedTable;
use write_fonts::read::{FontRef, TableProvider};
use write_fonts::tables::cmap::{Cmap, Cmap4, CmapSubtable, EncodingRecord, PlatformId};
use write_fonts::tables::gsub::{
    Gsub, Ligature, LigatureSet, LigatureSubstFormat1, SubstitutionLookup,
};
use write_fonts::tables::layout::{
    CoverageTable, Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag, LookupList,
    Script, ScriptList, ScriptRecord,
};
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::Os2;
use write_fonts::types::{GlyphId16, NameId, Tag};
use write_fonts::FontBuilder;

const CONVERTER_SCHEMA_VERSION: &str = "v11_verified_geetanjali_win1252_map";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum LegacyScriptFamily {
    RamdhenuAssamese, // GeetanjaliP, Asomiya, Borluit, Ramdhenu ASCII layout ('k' = ক, 'e' = া, 'f' = ি)
    GeetanjaliHighAscii, // Geetanjali, ABC Ganita/Handwriting/Helen/Laxminath ('K' = া, 0xDF = ক)
    BijoyBengali,     // SutonnyMJ, Bijoy, Sulekha ('K' = ক, 'v' = া, 'w' = ি)
    KrutiDevHindi,    // Kruti Dev, DevLys, Chanakya ('d' = क, 'k' = ा, 'f' = ि)
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

fn lookup_legacy_glyph(face: &Face, ch: char) -> Option<GlyphId> {
    let code = ch as u32;

    if code <= 0xFF {
        if let Some(gid) = cmap_glyph(face, ttf_parser::PlatformId::Macintosh, 0, code) {
            return Some(gid);
        }

        if let Some(symbol_code) = code.checked_add(0xF000) {
            if let Some(gid) = cmap_glyph(face, ttf_parser::PlatformId::Windows, 0, symbol_code) {
                return Some(gid);
            }
        }
    }

    if let Some(win1252_uni) = win1252_byte_to_unicode(code) {
        if let Some(gid) = face.glyph_index(win1252_uni) {
            if gid.0 != 0 {
                return Some(gid);
            }
        }
    }

    if let Some(gid) = face.glyph_index(ch) {
        if gid.0 != 0 {
            return Some(gid);
        }
    }

    None
}

fn cmap_glyph(
    face: &Face,
    platform_id: ttf_parser::PlatformId,
    encoding_id: u16,
    codepoint: u32,
) -> Option<GlyphId> {
    let cmap = face.tables().cmap?;
    cmap.subtables
        .into_iter()
        .find(|subtable| {
            subtable.platform_id == platform_id && subtable.encoding_id == encoding_id
        })?
        .glyph_index(codepoint)
        .filter(|glyph_id| glyph_id.0 != 0)
}

fn win1252_byte_to_unicode(b: u32) -> Option<char> {
    let cp = match b {
        0x80 => 0x20AC,
        0x82 => 0x201A,
        0x83 => 0x0192,
        0x84 => 0x201E,
        0x85 => 0x2026,
        0x86 => 0x2020,
        0x87 => 0x2021,
        0x88 => 0x02C6,
        0x89 => 0x2030,
        0x8A => 0x0160,
        0x8B => 0x2039,
        0x8C => 0x0152,
        0x8E => 0x017D,
        0x91 => 0x2018,
        0x92 => 0x2019,
        0x93 => 0x201C,
        0x94 => 0x201D,
        0x95 => 0x2022,
        0x96 => 0x2013,
        0x97 => 0x2014,
        0x98 => 0x02DC,
        0x99 => 0x2122,
        0x9A => 0x0161,
        0x9B => 0x203A,
        0x9C => 0x0153,
        0x9E => 0x017E,
        0x9F => 0x0178,
        _ => return None,
    };
    char::from_u32(cp)
}

fn find_invisible_glyph(face: &Face) -> Option<GlyphId> {
    (1..face.number_of_glyphs()).map(GlyphId).find(|&glyph_id| {
        face.glyph_hor_advance(glyph_id) == Some(0) && face.glyph_bounding_box(glyph_id).is_none()
    })
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

    let out_dir = get_windows_user_fonts_dir();
    let _ = fs::create_dir_all(&out_dir);

    let marker_dir = get_app_state_dir();
    let _ = fs::create_dir_all(&marker_dir);
    let version_marker = marker_dir.join(".schema_version");

    let needs_rebuild = fs::read_to_string(&version_marker)
        .map(|v| v.trim() != CONVERTER_SCHEMA_VERSION)
        .unwrap_or(true);

    if needs_rebuild {
        let old_dir = marker_dir.join("ConvertedFonts");
        if let Ok(existing) = fs::read_dir(&old_dir) {
            for entry in existing.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        if let Ok(existing) = fs::read_dir(&out_dir) {
            for entry in existing.flatten() {
                let p = entry.path();
                if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                    if fname.ends_with("_Unicode.ttf") || fname.ends_with("_Unicode.otf") {
                        let _ = fs::remove_file(p);
                    }
                }
            }
        }
        let _ = fs::write(&version_marker, CONVERTER_SCHEMA_VERSION);
    }

    let mut scan_dirs: Vec<PathBuf> = vec![PathBuf::from(r"C:\Windows\Fonts")];
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        let user_fonts = Path::new(&local_app).join(r"Microsoft\Windows\Fonts");
        if user_fonts.exists() {
            scan_dirs.push(user_fonts);
        }
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

            if ext != "ttf" && ext != "otf" {
                continue;
            }

            let Some(analysis) = classify_font_file(&path) else {
                continue;
            };

            match analysis {
                FontClassification::AlreadyUnicode {
                    display_name,
                    script,
                } => {
                    let key = display_name.to_ascii_lowercase();
                    if seen_families.insert(key.clone()) {
                        results.push(ConvertedFontInfo {
                            id: format!("uni_{}", key.replace(' ', "_")),
                            original_name: display_name.clone(),
                            unicode_family_name: display_name,
                            script_label: script,
                            output_path: path.to_string_lossy().to_string(),
                            newly_converted: false,
                        });
                    }
                }
                FontClassification::LegacyNeedsConversion {
                    display_name,
                    family_name,
                    subfamily_name,
                    legacy_type,
                    is_cff_otf,
                } => {
                    let unicode_name = format!("{} Unicode", family_name);
                    let unicode_display_name = font_display_name(&unicode_name, &subfamily_name);
                    let key = unicode_display_name.to_ascii_lowercase();
                    if !seen_families.insert(key.clone()) {
                        continue;
                    }

                    // Preserve .otf extension when source font uses PostScript CFF outlines (`OTTO`)
                    let out_ext = if is_cff_otf { "otf" } else { "ttf" };
                    let safe_filename = format!(
                        "{}_Unicode.{}",
                        unicode_display_name
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                            .collect::<String>(),
                        out_ext
                    );
                    let dest_path = out_dir.join(&safe_filename);

                    let mut newly_converted = false;
                    if !dest_path.exists() {
                        match convert_legacy_font_to_unicode(
                            &path,
                            &dest_path,
                            &unicode_name,
                            &subfamily_name,
                            &legacy_type,
                        ) {
                            Ok(()) => {
                                register_font_for_current_user(
                                    &dest_path,
                                    &unicode_display_name,
                                    is_cff_otf,
                                );
                                newly_converted = true;
                                installed_any_new = true;
                            }
                            Err(error) => {
                                eprintln!(
                                    "Failed to convert legacy font '{}': {error}",
                                    path.display()
                                );
                                continue;
                            }
                        }
                    } else {
                        register_font_for_current_user(
                            &dest_path,
                            &unicode_display_name,
                            is_cff_otf,
                        );
                    }

                    if dest_path.exists() {
                        let badge = match legacy_type {
                            LegacyScriptFamily::RamdhenuAssamese
                            | LegacyScriptFamily::GeetanjaliHighAscii => "ASM-UNI",
                            LegacyScriptFamily::BijoyBengali => "BNG-UNI",
                            LegacyScriptFamily::KrutiDevHindi => "HIN-UNI",
                        };

                        results.push(ConvertedFontInfo {
                            id: format!("conv_{}", key.replace(' ', "_")),
                            original_name: display_name,
                            unicode_family_name: unicode_display_name,
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

fn get_windows_user_fonts_dir() -> PathBuf {
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        Path::new(&local_app).join(r"Microsoft\Windows\Fonts")
    } else {
        PathBuf::from(r"C:\Windows\Fonts")
    }
}

fn get_app_state_dir() -> PathBuf {
    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        Path::new(&local_app).join("PranabKeyboard")
    } else {
        PathBuf::from(r"C:\Users\Public\PranabKeyboard")
    }
}

enum FontClassification {
    AlreadyUnicode {
        display_name: String,
        script: String,
    },
    LegacyNeedsConversion {
        display_name: String,
        family_name: String,
        subfamily_name: String,
        legacy_type: LegacyScriptFamily,
        is_cff_otf: bool,
    },
}

fn extract_font_names(face: &Face, path: &Path) -> Option<(String, String)> {
    let family = get_font_name(face, name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| get_font_name(face, name_id::FAMILY))
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_owned))?;

    let subfamily = get_font_name(face, name_id::TYPOGRAPHIC_SUBFAMILY)
        .or_else(|| get_font_name(face, name_id::SUBFAMILY))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Regular".to_string());

    Some((family.trim().to_string(), subfamily.trim().to_string()))
}

fn font_display_name(family: &str, subfamily: &str) -> String {
    let sub_trim = subfamily.trim();
    if !sub_trim.is_empty()
        && !sub_trim.eq_ignore_ascii_case("Regular")
        && !sub_trim.eq_ignore_ascii_case("Normal")
        && !family
            .to_ascii_lowercase()
            .contains(&sub_trim.to_ascii_lowercase())
    {
        format!("{} {}", family.trim(), sub_trim)
    } else {
        family.trim().to_string()
    }
}

fn get_font_name(face: &Face, id: u16) -> Option<String> {
    face.names()
        .into_iter()
        .find(|name| name.name_id == id && name.is_unicode())
        .and_then(|name| name.to_string())
        .or_else(|| {
            face.names()
                .into_iter()
                .find(|name| {
                    name.name_id == id && name.platform_id == ttf_parser::PlatformId::Macintosh
                })
                .map(|name| String::from_utf8_lossy(name.name).trim().to_string())
                .filter(|name| !name.is_empty())
        })
}

fn classify_font_file(path: &Path) -> Option<FontClassification> {
    let data = fs::read(path).ok()?;
    let face = Face::parse(&data, 0).ok()?;
    let is_cff_otf = data.starts_with(b"OTTO");

    let (family_name, subfamily_name) = extract_font_names(&face, path)?;
    let display_name = font_display_name(&family_name, &subfamily_name);
    let lower = display_name.to_ascii_lowercase();

    if lower.ends_with(" unicode")
        || [
            "segoe",
            "arial",
            "calibri",
            "cambria",
            "consolas",
            "courier",
            "georgia",
            "impact",
            "lucida",
            "microsoft",
            "palatino",
            "tahoma",
            "times",
            "trebuchet",
            "verdana",
            "wingdings",
            "webdings",
            "symbol",
            "marlett",
            "gabriola",
            "ebrima",
            "gadugi",
            "malgun",
            "meiryo",
            "mingliu",
            "simsun",
            "sylfaen",
            "yu gothic",
            "cascadia",
            "bahnschrift",
            "ink free",
            "mv boli",
            "ms gothic",
            "ms pgothic",
        ]
        .iter()
        .any(|sys| lower.starts_with(sys))
    {
        return None;
    }

    // 1. Native Unicode Indic font check
    let has_bengali_ka = face.glyph_index('ক').is_some();
    let has_hindi_ka = face.glyph_index('क').is_some();

    if has_bengali_ka || has_hindi_ka {
        let script = if has_bengali_ka { "ASM/BNG" } else { "HINDI" };
        return Some(FontClassification::AlreadyUnicode {
            display_name,
            script: script.into(),
        });
    }

    // 2. Inspect actual glyph geometry FIRST
    if let Some(detected) = detect_legacy_encoding_by_glyphs(&face) {
        return Some(FontClassification::LegacyNeedsConversion {
            display_name: display_name.clone(),
            family_name: family_name.clone(),
            subfamily_name: subfamily_name.clone(),
            legacy_type: detected,
            is_cff_otf,
        });
    }

    // 3. Fallback family-name heuristic match
    if (lower.contains("geetanjali") && !lower.contains("geetanjalip")) || lower.starts_with("abc ")
    {
        return Some(FontClassification::LegacyNeedsConversion {
            display_name: display_name.clone(),
            family_name: family_name.clone(),
            subfamily_name: subfamily_name.clone(),
            legacy_type: LegacyScriptFamily::GeetanjaliHighAscii,
            is_cff_otf,
        });
    }

    if [
        "geetanjalip",
        "asomiya",
        "borluit",
        "ramdhenu",
        "brahmaputra",
        "pragjyotish",
        "luit",
        "lakhimi",
        "axom",
        "rupali",
        "amar",
        "surma",
        "barak",
        "kopili",
        "manas",
        "kaziranga",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            display_name,
            family_name,
            subfamily_name,
            legacy_type: LegacyScriptFamily::RamdhenuAssamese,
            is_cff_otf,
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
        "dhaka",
        "bangla",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            display_name,
            family_name,
            subfamily_name,
            legacy_type: LegacyScriptFamily::BijoyBengali,
            is_cff_otf,
        });
    }

    if ["kruti", "devlys", "chanakya", "agra", "kundli", "shusha"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        return Some(FontClassification::LegacyNeedsConversion {
            display_name,
            family_name,
            subfamily_name,
            legacy_type: LegacyScriptFamily::KrutiDevHindi,
            is_cff_otf,
        });
    }

    None
}

fn detect_legacy_encoding_by_glyphs(face: &Face) -> Option<LegacyScriptFamily> {
    let k_gid = lookup_legacy_glyph(face, 'k')?;
    let e_gid = lookup_legacy_glyph(face, 'e')?;
    let f_gid = lookup_legacy_glyph(face, 'f')?;

    let k_bbox = face.glyph_bounding_box(k_gid);
    let e_bbox = face.glyph_bounding_box(e_gid);
    let f_bbox = face.glyph_bounding_box(f_gid);

    if let (Some(kb), Some(eb), Some(fb)) = (k_bbox, e_bbox, f_bbox) {
        let k_w = (kb.x_max - kb.x_min).max(1) as f32;
        let e_w = (eb.x_max - eb.x_min).max(1) as f32;
        let f_h = (fb.y_max - fb.y_min).max(1) as f32;
        let k_h = (kb.y_max - kb.y_min).max(1) as f32;

        // 1. Ramdhenu Assamese (e.g. GeetanjaliP): 'e' is narrow vertical aa-matra (া), 'f' is tall i-matra (ি)
        if e_w < k_w * 0.55 && f_h > k_h * 1.15 {
            if lookup_legacy_glyph(face, 'j').is_some() && lookup_legacy_glyph(face, 'w').is_some()
            {
                return Some(LegacyScriptFamily::RamdhenuAssamese);
            }
        }

        // 2. Kruti Dev Hindi: 'k' is narrow aa-matra (ा), 'f' is tall i-matra (ि)
        if k_w < e_w * 0.55 && f_h > k_h * 1.15 {
            return Some(LegacyScriptFamily::KrutiDevHindi);
        }

        // 3. Bijoy / ANSI Layout: 'v' is narrow aa-matra (া), 'w' is tall pre-base i-matra (ি)
        if let (Some(v_gid), Some(w_gid)) = (
            lookup_legacy_glyph(face, 'v'),
            lookup_legacy_glyph(face, 'w'),
        ) {
            if let (Some(vb), Some(wb)) = (
                face.glyph_bounding_box(v_gid),
                face.glyph_bounding_box(w_gid),
            ) {
                let v_w = (vb.x_max - vb.x_min).max(1) as f32;
                let w_h = (wb.y_max - wb.y_min).max(1) as f32;
                if v_w < k_w * 0.55 && w_h > k_h * 1.15 {
                    return Some(LegacyScriptFamily::BijoyBengali);
                }
            }
        }

        // 4. Geetanjali / ABC High-ASCII Assam DTP Layout:
        // 'K' (0x4B) is the narrow vertical aa-matra (া) while 'k' and 'e' are wide glyphs
        if let Some(cap_k_gid) = lookup_legacy_glyph(face, 'K') {
            if let Some(cap_kb) = face.glyph_bounding_box(cap_k_gid) {
                let cap_k_w = (cap_kb.x_max - cap_kb.x_min).max(1) as f32;
                if cap_k_w < k_w * 0.50 && e_w > cap_k_w * 1.5 {
                    return Some(LegacyScriptFamily::GeetanjaliHighAscii);
                }
            }
        }
    }

    None
}

fn convert_legacy_font_to_unicode(
    src_path: &Path,
    dest_path: &Path,
    new_family_name: &str,
    new_subfamily_name: &str,
    legacy_type: &LegacyScriptFamily,
) -> Result<(), String> {
    let raw_bytes = fs::read(src_path).map_err(|e| e.to_string())?;
    let parsed_face = Face::parse(&raw_bytes, 0).map_err(|e| e.to_string())?;
    let font_ref = FontRef::new(&raw_bytes).map_err(|e| e.to_string())?;

    let legacy_pairs = get_legacy_to_unicode_table(legacy_type);
    validate_legacy_mapping(&legacy_pairs)?;
    let mut unicode_to_gid: BTreeMap<u32, u16> = BTreeMap::new();

    let space_gid = lookup_legacy_glyph(&parsed_face, ' ')
        .map(|glyph_id| glyph_id.0)
        .ok_or_else(|| "Legacy font does not contain a usable space glyph".to_string())?;
    unicode_to_gid.insert(0x0020, space_gid);
    unicode_to_gid.insert(0x00A0, space_gid);

    for ascii_ch in ['.', ',', '-', '(', ')', '?', '!', ':', ';', '/', '\'', '"'] {
        if let Some(gid) = lookup_legacy_glyph(&parsed_face, ascii_ch) {
            unicode_to_gid.insert(ascii_ch as u32, gid.0);
        }
    }

    for (legacy_ch, unicode_ch) in legacy_pairs {
        if let Some(gid) = lookup_legacy_glyph(&parsed_face, legacy_ch) {
            unicode_to_gid.insert(unicode_ch as u32, gid.0);
        }
    }

    if *legacy_type != LegacyScriptFamily::KrutiDevHindi {
        if let Some(&ra_gid) = unicode_to_gid.get(&('ৰ' as u32)) {
            unicode_to_gid.entry('র' as u32).or_insert(ra_gid);
        } else if let Some(&ra_gid) = unicode_to_gid.get(&('র' as u32)) {
            unicode_to_gid.entry('ৰ' as u32).or_insert(ra_gid);
        }

        if let Some(&ba_gid) = unicode_to_gid.get(&('ব' as u32)) {
            unicode_to_gid.entry('ৱ' as u32).or_insert(ba_gid);
        }
        if let Some(&da_gid) = unicode_to_gid.get(&('ড' as u32)) {
            unicode_to_gid.entry('ড়' as u32).or_insert(da_gid);
        }
        if let Some(&dha_gid) = unicode_to_gid.get(&('ঢ' as u32)) {
            unicode_to_gid.entry('ঢ়' as u32).or_insert(dha_gid);
        }
        if let Some(&ya_gid) = unicode_to_gid.get(&('য' as u32)) {
            unicode_to_gid.entry('য়' as u32).or_insert(ya_gid);
        }
        if let Some(&ta_gid) = unicode_to_gid.get(&('ত' as u32)) {
            unicode_to_gid.entry('ৎ' as u32).or_insert(ta_gid);
        }
        if let Some(&e_matra_gid) = unicode_to_gid.get(&('ে' as u32)) {
            unicode_to_gid.entry('ো' as u32).or_insert(e_matra_gid);
            unicode_to_gid.entry('ৌ' as u32).or_insert(e_matra_gid);
        }
    }

    if let Some(invisible_gid) = find_invisible_glyph(&parsed_face) {
        unicode_to_gid.entry(0x200C).or_insert(invisible_gid.0);
        unicode_to_gid.entry(0x200D).or_insert(invisible_gid.0);
    }

    if let Some(dotted_circle_gid) = parsed_face.glyph_index('\u{25CC}') {
        if dotted_circle_gid.0 != 0 {
            unicode_to_gid.entry(0x25CC).or_insert(dotted_circle_gid.0);
        }
    }

    let cmap = build_unicode_cmap(&unicode_to_gid);
    let name_table = build_unicode_name_table(new_family_name, new_subfamily_name);
    let gsub_table = build_indic_gsub(&parsed_face, &unicode_to_gid, legacy_type);

    let mut builder = FontBuilder::new();
    builder.add_table(&cmap).map_err(|e| e.to_string())?;
    builder.add_table(&name_table).map_err(|e| e.to_string())?;
    if let Some(gsub_table) = &gsub_table {
        builder.add_table(gsub_table).map_err(|e| e.to_string())?;
    }

    let os2_ref = font_ref
        .os2()
        .map_err(|error| format!("Legacy font has no usable OS/2 table: {error}"))?;
    let mut os2_owned: Os2 = os2_ref.to_owned_table();
    os2_owned.ul_unicode_range_1 |= 1 << 0;
    match legacy_type {
        LegacyScriptFamily::RamdhenuAssamese
        | LegacyScriptFamily::GeetanjaliHighAscii
        | LegacyScriptFamily::BijoyBengali => {
            os2_owned.ul_unicode_range_1 |= 1 << 16;
        }
        LegacyScriptFamily::KrutiDevHindi => {
            os2_owned.ul_unicode_range_1 |= 1 << 15;
        }
    }
    builder.add_table(&os2_owned).map_err(|e| e.to_string())?;

    for record in font_ref.table_directory.table_records() {
        let tag = record.tag();
        if tag == Tag::new(b"cmap")
            || tag == Tag::new(b"name")
            || tag == Tag::new(b"OS/2")
            || tag == Tag::new(b"DSIG")
            || (gsub_table.is_some() && tag == Tag::new(b"GSUB"))
        {
            continue;
        }
        if let Some(table_data) = font_ref.table_data(tag) {
            builder.add_raw(tag, table_data);
        }
    }

    let mut built_bytes = builder.build();
    let sfnt_signature = raw_bytes
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Legacy font has an invalid sfnt header".to_string())?;
    finalize_sfnt(&mut built_bytes, sfnt_signature)?;
    Face::parse(&built_bytes, 0)
        .map_err(|error| format!("Converted font failed validation: {error:?}"))?;
    fs::write(dest_path, built_bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_legacy_mapping(pairs: &[(char, char)]) -> Result<(), String> {
    let mut mappings = BTreeMap::new();
    for &(legacy_character, unicode_character) in pairs {
        if let Some(previous) = mappings.insert(legacy_character, unicode_character) {
            if previous != unicode_character {
                return Err(format!(
                    "Legacy mapping is ambiguous: {:?} maps to both U+{:04X} and U+{:04X}",
                    legacy_character, previous as u32, unicode_character as u32
                ));
            }
        }
    }
    Ok(())
}

fn finalize_sfnt(bytes: &mut [u8], signature: [u8; 4]) -> Result<(), String> {
    const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;

    if signature != [0, 1, 0, 0] && signature != *b"OTTO" {
        return Err("Unsupported sfnt outline signature".to_string());
    }
    if bytes.len() < 12 {
        return Err("Converted font has a truncated sfnt header".to_string());
    }

    bytes[..4].copy_from_slice(&signature);
    let table_count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let directory_end = 12usize
        .checked_add(
            table_count
                .checked_mul(16)
                .ok_or("Invalid sfnt table count")?,
        )
        .ok_or("Invalid sfnt table directory")?;
    if directory_end > bytes.len() {
        return Err("Converted font has a truncated table directory".to_string());
    }

    let mut head_range = None;
    for index in 0..table_count {
        let record_offset = 12 + index * 16;
        if bytes[record_offset..record_offset + 4] == *b"head" {
            let table_offset = u32::from_be_bytes(
                bytes[record_offset + 8..record_offset + 12]
                    .try_into()
                    .map_err(|_| "Invalid head table offset")?,
            ) as usize;
            let table_length = u32::from_be_bytes(
                bytes[record_offset + 12..record_offset + 16]
                    .try_into()
                    .map_err(|_| "Invalid head table length")?,
            ) as usize;
            let table_end = table_offset
                .checked_add(table_length)
                .ok_or("Invalid head table range")?;
            if table_length < 12 || table_end > bytes.len() {
                return Err("Converted font has an invalid head table".to_string());
            }
            if head_range.replace((table_offset, table_end)).is_some() {
                return Err("Converted font contains multiple head tables".to_string());
            }
        }
    }

    let (head_table_offset, _) =
        head_range.ok_or_else(|| "Converted font has no head table".to_string())?;
    bytes[head_table_offset + 8..head_table_offset + 12].fill(0);

    for index in 0..table_count {
        let record_offset = 12 + index * 16;
        let table_offset = u32::from_be_bytes(
            bytes[record_offset + 8..record_offset + 12]
                .try_into()
                .map_err(|_| "Invalid sfnt table offset")?,
        ) as usize;
        let table_length = u32::from_be_bytes(
            bytes[record_offset + 12..record_offset + 16]
                .try_into()
                .map_err(|_| "Invalid sfnt table length")?,
        ) as usize;
        let table_end = table_offset
            .checked_add(table_length)
            .ok_or("Invalid sfnt table range")?;
        if table_end > bytes.len() {
            return Err("Converted font contains a table outside the file".to_string());
        }

        let checksum = sfnt_checksum(&bytes[table_offset..table_end]);
        bytes[record_offset + 4..record_offset + 8].copy_from_slice(&checksum.to_be_bytes());
    }

    let adjustment = CHECKSUM_MAGIC.wrapping_sub(sfnt_checksum(bytes));
    bytes[head_table_offset + 8..head_table_offset + 12].copy_from_slice(&adjustment.to_be_bytes());

    if sfnt_checksum(bytes) != CHECKSUM_MAGIC {
        return Err("Converted font checksum finalization failed".to_string());
    }
    Ok(())
}

fn sfnt_checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn get_legacy_to_unicode_table(legacy_type: &LegacyScriptFamily) -> Vec<(char, char)> {
    match legacy_type {
        // Verified directly from your Photoshop layer ("ß Á à á â ã Ä Å" -> ক খ গ ঘ ঙ চ ছ জ)
        LegacyScriptFamily::GeetanjaliHighAscii => vec![
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
            ('\u{00DF}', 'ক'), // ß
            ('\u{00C1}', 'খ'), // Á
            ('\u{00E0}', 'গ'), // à
            ('\u{00E1}', 'ঘ'), // á
            ('\u{00E2}', 'ঙ'), // â
            ('\u{00E3}', 'চ'), // ã
            ('\u{00C4}', 'ছ'), // Ä
            ('\u{00C5}', 'জ'), // Å
            ('\u{00E6}', 'ঝ'), // æ
            ('\u{00E7}', 'ঞ'), // ç
            ('\u{00E8}', 'ট'), // è
            ('\u{00E9}', 'ঠ'), // é
            ('\u{00EA}', 'ড'), // ê
            ('\u{00EB}', 'ঢ'), // ë
            ('\u{00EC}', 'ণ'), // ì
            ('\u{00ED}', 'ত'), // í
            ('\u{00EE}', 'থ'), // î
            ('\u{00EF}', 'দ'), // ï
            ('\u{00F0}', 'ধ'), // ð
            ('\u{00F1}', 'ন'), // ñ
            ('\u{00F2}', 'প'), // ò
            ('\u{00F3}', 'ফ'), // ó
            ('\u{00F4}', 'ব'), // ô
            ('\u{00F5}', 'ভ'), // õ
            ('\u{00F6}', 'ম'), // ö
            ('\u{00F8}', 'য'), // ø
            ('\u{00F9}', 'ৰ'), // ù
            ('\u{00FA}', 'ল'), // ú
            ('\u{00FB}', 'ৱ'), // û
            ('\u{00FC}', 'শ'), // ü
            ('\u{00FD}', 'ষ'), // ý
            ('\u{00FE}', 'স'), // þ
            ('\u{00FF}', 'হ'), // ÿ
            ('/', 'য়'),
            ('|', 'ৎ'),
            ('K', 'া'),
            ('L', 'ি'),
            ('M', 'ী'),
            ('N', 'ু'),
            ('O', 'ূ'),
            ('P', 'ৃ'),
            ('Q', 'ে'),
            ('R', 'ৈ'),
            ('S', 'ো'),
            ('T', 'ৌ'),
            ('J', '্'),
            ('V', 'ং'),
            ('W', 'ঃ'),
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

fn build_unicode_cmap(map: &BTreeMap<u32, u16>) -> Cmap {
    let mut start_codes: Vec<u16> = Vec::new();
    let mut end_codes: Vec<u16> = Vec::new();
    let mut id_deltas: Vec<i16> = Vec::new();
    let mut id_range_offsets: Vec<u16> = Vec::new();

    for (&cp, &gid) in map {
        if cp <= 0xFFFE {
            let code = cp as u16;
            start_codes.push(code);
            end_codes.push(code);
            let delta = (gid as i32 - code as i32) as i16;
            id_deltas.push(delta);
            id_range_offsets.push(0);
        }
    }

    start_codes.push(0xFFFF);
    end_codes.push(0xFFFF);
    id_deltas.push(1);
    id_range_offsets.push(0);

    let format4 = Cmap4::new(
        0,
        end_codes,
        start_codes,
        id_deltas,
        id_range_offsets,
        vec![],
    );

    let subtable = CmapSubtable::Format4(format4);
    let records = vec![
        EncodingRecord::new(PlatformId::Unicode, 3, subtable.clone()),
        EncodingRecord::new(PlatformId::Windows, 1, subtable),
    ];

    Cmap::new(records)
}

fn build_unicode_name_table(family_name: &str, subfamily_name: &str) -> Name {
    let full_name = font_display_name(family_name, subfamily_name);
    let ps_name: String = full_name.chars().filter(|c| !c.is_whitespace()).collect();
    let unique_id = format!("PranabKeyboard: {}: {}", family_name, subfamily_name);
    let records = vec![
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::FAMILY_NAME,
            family_name.to_string().into(),
        ),
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::SUBFAMILY_NAME,
            subfamily_name.to_string().into(),
        ),
        NameRecord::new(3, 1, 0x0409, NameId::UNIQUE_ID, unique_id.into()),
        NameRecord::new(3, 1, 0x0409, NameId::FULL_NAME, full_name.clone().into()),
        NameRecord::new(3, 1, 0x0409, NameId::POSTSCRIPT_NAME, ps_name.into()),
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::TYPOGRAPHIC_FAMILY_NAME,
            family_name.to_string().into(),
        ),
        NameRecord::new(
            3,
            1,
            0x0409,
            NameId::TYPOGRAPHIC_SUBFAMILY_NAME,
            subfamily_name.to_string().into(),
        ),
    ];
    Name::new(records)
}

fn build_indic_gsub(
    face: &Face,
    unicode_to_gid: &BTreeMap<u32, u16>,
    legacy_type: &LegacyScriptFamily,
) -> Option<Gsub> {
    if *legacy_type != LegacyScriptFamily::RamdhenuAssamese {
        return None;
    }

    let conjunct_specs: &[(&[char], char)] = &[
        (&['ক', '্', 'ষ'], '&'),
        (&['জ', '্', 'ঞ'], '*'),
        (&['ত', '্', 'ৰ'], '^'),
        (&['ত', '্', 'র'], '^'),
        (&['হ', '্', 'ম'], '!'),
        (&['ঙ', '্', 'গ'], 'N'),
    ];

    let mut lig_groups: BTreeMap<GlyphId16, Vec<Ligature>> = BTreeMap::new();

    for &(sequence, legacy_ligature_char) in conjunct_specs {
        if let Some(GlyphId(ligature_gid)) = lookup_legacy_glyph(face, legacy_ligature_char) {
            let mut glyphs = Vec::with_capacity(sequence.len());
            for &character in sequence {
                if let Some(&glyph_id) = unicode_to_gid.get(&(character as u32)) {
                    glyphs.push(GlyphId16::new(glyph_id));
                }
            }
            if glyphs.len() == sequence.len() && glyphs.len() >= 2 {
                let first_glyph = glyphs[0];
                lig_groups
                    .entry(first_glyph)
                    .or_default()
                    .push(Ligature::new(
                        GlyphId16::new(ligature_gid),
                        glyphs[1..].to_vec(),
                    ));
            }
        }
    }

    if lig_groups.is_empty() {
        return None;
    }

    let mut coverage_glyphs = Vec::new();
    let mut ligature_sets = Vec::new();
    for (first_glyph, mut ligatures) in lig_groups {
        ligatures.sort_by_key(|ligature| std::cmp::Reverse(ligature.component_glyph_ids.len()));
        coverage_glyphs.push(first_glyph);
        ligature_sets.push(LigatureSet::new(ligatures));
    }

    let coverage: CoverageTable = coverage_glyphs.into_iter().collect();
    let ligature_substitution = LigatureSubstFormat1::new(coverage, ligature_sets);
    let ligature_lookup = SubstitutionLookup::Ligature(Lookup::new(
        LookupFlag::empty(),
        vec![ligature_substitution],
    ));

    let mut feature_records = vec![
        FeatureRecord::new(Tag::new(b"akhn"), Feature::new(None, vec![0])),
        FeatureRecord::new(Tag::new(b"cjct"), Feature::new(None, vec![0])),
    ];

    feature_records.sort_by_key(|r| r.feature_tag);
    let feature_indices: Vec<u16> = (0..feature_records.len() as u16).collect();

    let lookup_list = LookupList::new(vec![ligature_lookup]);
    let feature_list = FeatureList::new(feature_records);

    let lang_sys = LangSys::new(feature_indices);
    let script = Script::new(Some(lang_sys), vec![]);
    let script_tag = match legacy_type {
        LegacyScriptFamily::KrutiDevHindi => Tag::new(b"dev2"),
        _ => Tag::new(b"bng2"),
    };
    let script_list = ScriptList::new(vec![ScriptRecord::new(script_tag, script)]);

    Some(Gsub::new(script_list, feature_list, lookup_list))
}

fn register_font_for_current_user(font_path: &Path, display_name: &str, is_cff_otf: bool) {
    register_font_session_only(font_path);

    let font_type = if is_cff_otf { "OpenType" } else { "TrueType" };
    let reg_key = format!("{} ({})", display_name, font_type);
    let path_str = font_path.to_string_lossy().to_string();

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

fn register_font_session_only(font_path: &Path) {
    let mut wide: Vec<u16> = font_path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = AddFontResourceExW(PCWSTR(wide.as_mut_ptr()), Default::default(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sfnt_finalization_sets_outline_signature_and_checksum() {
        for signature in [[0, 1, 0, 0], *b"OTTO"] {
            let mut font = vec![0; 44];
            font[..4].copy_from_slice(&signature);
            font[4..6].copy_from_slice(&1u16.to_be_bytes());
            font[12..16].copy_from_slice(b"head");
            font[20..24].copy_from_slice(&28u32.to_be_bytes());
            font[24..28].copy_from_slice(&16u32.to_be_bytes());
            font[28..32].copy_from_slice(&0x0001_0000u32.to_be_bytes());

            finalize_sfnt(&mut font, signature).expect("well-formed sfnt should finalize");

            assert_eq!(&font[..4], &signature);
            assert_eq!(sfnt_checksum(&font), 0xB1B0_AFBA);
            let head_checksum = u32::from_be_bytes(font[16..20].try_into().unwrap());
            let mut head = font[28..44].to_vec();
            head[8..12].fill(0);
            assert_eq!(head_checksum, sfnt_checksum(&head));
        }
    }

    #[test]
    fn windows_1252_c1_bytes_decode_without_guessing_undefined_slots() {
        assert_eq!(win1252_byte_to_unicode(0x80), Some('€'));
        assert_eq!(win1252_byte_to_unicode(0x91), Some('‘'));
        assert_eq!(win1252_byte_to_unicode(0x81), None);
    }

    #[test]
    fn ambiguous_legacy_codes_are_rejected() {
        let error = validate_legacy_mapping(&[('Q', 'ঔ'), ('Q', 'ে')])
            .expect_err("conflicting source codes must not be silently mapped");
        assert!(error.contains("ambiguous"));
    }
}
