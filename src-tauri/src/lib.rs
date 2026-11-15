use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_MENU, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
    TranslateMessage, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

#[derive(Clone, Serialize)]
pub struct ImeState {
    pub enabled: bool,
    pub layout: String,   // "phonetic" | "smart_assamese" | "ai_mode"
    pub encoding: String, // "unicode" | "legacy"
}

struct EngineContext {
    state: ImeState,
    buffer: String,
    word_raw: String,
    prev_was_consonant: bool,
    last_unicode_char: String,
    app_handle: Option<AppHandle>,
}

static ENGINE: Lazy<Mutex<EngineContext>> = Lazy::new(|| {
    Mutex::new(EngineContext {
        state: ImeState {
            enabled: false,
            layout: "phonetic".into(),
            encoding: "unicode".into(),
        },
        buffer: String::new(),
        word_raw: String::new(),
        prev_was_consonant: false,
        last_unicode_char: String::new(),
        app_handle: None,
    })
});

static HOOK_HANDLE_RAW: AtomicIsize = AtomicIsize::new(0);
static IS_INJECTING: AtomicBool = AtomicBool::new(false);

fn get_hook_handle() -> HHOOK {
    HHOOK(HOOK_HANDLE_RAW.load(Ordering::SeqCst) as *mut core::ffi::c_void)
}

fn is_key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

fn send_text(text: &str, backspaces: usize) {
    IS_INJECTING.store(true, Ordering::SeqCst);
    let mut inputs: Vec<INPUT> = Vec::new();

    for _ in 0..backspaces {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK,
                    wScan: 0,
                    dwFlags: Default::default(),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    for code_unit in text.encode_utf16() {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    if !inputs.is_empty() {
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
    IS_INJECTING.store(false, Ordering::SeqCst);
}

fn vk_to_char(vk: u32, shift: bool) -> Option<char> {
    match vk {
        0x41..=0x5A => {
            let base = (vk as u8) as char;
            Some(if shift {
                base
            } else {
                base.to_ascii_lowercase()
            })
        }
        0x30..=0x39 => {
            let shifted = [')', '!', '@', '#', '$', '%', '^', '&', '*', '('];
            let idx = (vk - 0x30) as usize;
            Some(if shift {
                shifted[idx]
            } else {
                (vk as u8) as char
            })
        }
        0xC0 => Some(if shift { '~' } else { '`' }),
        0xBA => Some(if shift { ':' } else { ';' }),
        0xBB => Some(if shift { '+' } else { '=' }),
        0xBC => Some(if shift { '<' } else { ',' }),
        0xBD => Some(if shift { '_' } else { '-' }),
        0xBE => Some(if shift { '>' } else { '.' }),
        0xBF => Some(if shift { '?' } else { '/' }),
        0xDB => Some(if shift { '{' } else { '[' }),
        0xDD => Some(if shift { '}' } else { ']' }),
        0xDE => Some(if shift { '"' } else { '\'' }),
        0xDC => Some(if shift { '|' } else { '\\' }),
        _ => None,
    }
}

fn unicode_to_legacy_ascii(uni: &str) -> String {
    let mut out = String::new();
    for ch in uni.chars() {
        let mapped = match ch {
            'অ' => "A",
            'আ' => "Av",
            'ই' => "B",
            'ঈ' => "C",
            'উ' => "D",
            'ঊ' => "E",
            'ঋ' => "F",
            'এ' => "G",
            'ঐ' => "H",
            'ও' => "I",
            'ঔ' => "J",
            'ক' => "K",
            'খ' => "L",
            'গ' => "M",
            'ঘ' => "N",
            'ঙ' => "O",
            'চ' => "P",
            'ছ' => "Q",
            'জ' => "R",
            'ঝ' => "S",
            'ঞ' => "T",
            'ট' => "U",
            'ঠ' => "V",
            'ড' => "W",
            'ঢ' => "X",
            'ণ' => "Y",
            'ত' => "Z",
            'থ' => "_",
            'দ' => "`",
            'ধ' => "a",
            'ন' => "b",
            'প' => "c",
            'ফ' => "d",
            'ব' => "e",
            'ভ' => "f",
            'ম' => "g",
            'য' => "h",
            'ৰ' => "i",
            'ল' => "j",
            'ৱ' => "k",
            'শ' => "l",
            'ষ' => "m",
            'স' => "n",
            'হ' => "o",
            'ড়' => "p",
            'ঢ়' => "q",
            'য়' => "r",
            'ৎ' => "r",
            'া' => "v",
            'ি' => "w",
            'ী' => "x",
            'ু' => "y",
            'ূ' => "~",
            'ৃ' => "…",
            'ে' => "‡",
            'ৈ' => "‰",
            'ো' => "‡v",
            'ৌ' => "‡Š",
            '্' => "&",
            'ং' => "s",
            'ঃ' => "t",
            'ঁ' => "u",
            '।' => "|",
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
            other => {
                out.push(other);
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

fn translate_chart_phonetic(ch: char) -> Option<(&'static str, bool)> {
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

fn translate_smart_assamese(
    combo: &str,
    single: char,
    after_consonant: bool,
) -> Option<(String, usize, bool)> {
    let digraph = match combo {
        "kh" => Some(("খ", 1, true)),
        "gh" => Some(("ঘ", 1, true)),
        "ng" => Some(("ঙ", 1, true)),
        "ch" => Some(("ছ", 1, true)),
        "jh" => Some(("ঝ", 1, true)),
        "Th" => Some(("ঠ", 1, true)),
        "Dh" => Some(("ঢ", 1, true)),
        "th" => Some(("থ", 1, true)),
        "dh" => Some(("ধ", 1, true)),
        "ph" => Some(("ফ", 1, true)),
        "bh" => Some(("ভ", 1, true)),
        "sh" => Some(("শ", 1, true)),
        "Sh" => Some(("ষ", 1, true)),
        "Rh" => Some(("ঢ়", 1, true)),
        "kS" => Some(("ক্ষ", 1, true)),
        "aa" => Some((
            if after_consonant { "া" } else { "আ" },
            if after_consonant { 0 } else { 1 },
            false,
        )),
        "ee" | "ii" => Some((if after_consonant { "ী" } else { "ঈ" }, 1, false)),
        "oo" | "uu" => Some((if after_consonant { "ূ" } else { "ঊ" }, 1, false)),
        "oi" => Some((if after_consonant { "ৈ" } else { "ঐ" }, 1, false)),
        "ou" => Some((if after_consonant { "ৌ" } else { "ঔ" }, 1, false)),
        _ => None,
    };

    if let Some((text, bs, is_cons)) = digraph {
        return Some((text.to_string(), bs, is_cons));
    }

    if single == 'o' && after_consonant {
        return Some((String::new(), 0, false));
    }

    let consonant = match single {
        'k' => Some("ক"),
        'g' => Some("গ"),
        'c' => Some("চ"),
        'j' => Some("জ"),
        'T' => Some("ট"),
        'D' => Some("ড"),
        'N' => Some("ণ"),
        't' => Some("ত"),
        'd' => Some("দ"),
        'n' => Some("ন"),
        'p' => Some("প"),
        'f' => Some("ফ"),
        'b' => Some("ব"),
        'v' => Some("ভ"),
        'm' => Some("ম"),
        'z' => Some("য"),
        'r' => Some("ৰ"),
        'l' => Some("ল"),
        'w' => Some("ৱ"),
        's' => Some("স"),
        'h' => Some("হ"),
        'R' => Some("ড়"),
        'y' => Some("য়"),
        'x' => Some("ক্ষ"),
        _ => None,
    };

    if let Some(cons_str) = consonant {
        if after_consonant {
            return Some((format!("্{}", cons_str), 0, true));
        } else {
            return Some((cons_str.to_string(), 0, true));
        }
    }

    let other = match single {
        'a' => Some(if after_consonant { "া" } else { "অ" }),
        'i' => Some(if after_consonant { "ি" } else { "ই" }),
        'u' => Some(if after_consonant { "ু" } else { "উ" }),
        'e' => Some(if after_consonant { "ে" } else { "এ" }),
        'o' => Some("ও"),
        'O' => Some(if after_consonant { "ো" } else { "ও" }),
        'H' => Some("ঃ"),
        'M' => Some("ং"),
        '^' => Some("ঁ"),
        '/' | '\\' => Some("্"),
        '.' => Some("।"),
        '0' => Some("০"),
        '1' => Some("১"),
        '2' => Some("২"),
        '3' => Some("৩"),
        '4' => Some("৪"),
        '5' => Some("৫"),
        '6' => Some("৬"),
        '7' => Some("৭"),
        '8' => Some("৮"),
        '9' => Some("৯"),
        _ => None,
    };

    other.map(|s| (s.to_string(), 0, false))
}

fn translate_ai_mode(
    word_raw: &str,
    combo: &str,
    single: char,
    after_consonant: bool,
) -> Option<(String, usize, bool)> {
    match combo {
        "ksh" | "x" => return Some(("ক্ষ".to_string(), if combo.len() == 3 { 2 } else { 0 }, true)),
        "jny" | "gya" => return Some(("জ্ঞ".to_string(), 2, true)),
        "shr" => return Some(("শ্ৰ".to_string(), 2, true)),
        "tr" if !after_consonant => return Some(("ত্ৰ".to_string(), 1, true)),
        "ngk" => return Some(("ঙ্ক".to_string(), 2, true)),
        "ngg" => return Some(("ঙ্গ".to_string(), 2, true)),
        "hm" => return Some(("হ্ম".to_string(), 1, true)),
        "ri" if !after_consonant => return Some(("ঋ".to_string(), 1, false)),
        _ => {}
    }

    match single {
        '&' => return Some(("ক্ষ".to_string(), 0, true)),
        '*' => return Some(("জ্ঞ".to_string(), 0, true)),
        '^' if after_consonant => return Some(("ঁ".to_string(), 0, false)),
        '^' => return Some(("ত্ৰ".to_string(), 0, true)),
        '!' => return Some(("হ্ম".to_string(), 0, true)),
        '~' => return Some(("‘".to_string(), 0, false)),
        '@' => return Some(("’".to_string(), 0, false)),
        '|' => return Some(("ৎ".to_string(), 0, false)),
        '[' => return Some(("ড".to_string(), 0, true)),
        '{' => return Some(("ঢ".to_string(), 0, true)),
        '}' => return Some(("ঞ".to_string(), 0, true)),
        ']' => return Some(("়".to_string(), 0, false)),
        '>' => return Some(("।".to_string(), 0, false)),
        _ => {}
    }

    if single == 'd'
        && after_consonant
        && (word_raw.ends_with("kd")
            || word_raw.ends_with("gd")
            || word_raw.ends_with("td")
            || word_raw.ends_with("pd")
            || word_raw.ends_with("md")
            || word_raw.ends_with("sd"))
    {
        return Some(("্".to_string(), 0, false));
    }

    translate_smart_assamese(combo, single, after_consonant)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && !IS_INJECTING.load(Ordering::SeqCst)
        && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN)
    {
        let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kb.vkCode;
        let ctrl = is_key_down(VK_CONTROL);
        let alt = is_key_down(VK_MENU);
        let shift = is_key_down(VK_SHIFT);

        if (ctrl && vk == VK_SPACE.0 as u32)
            || (ctrl && alt && shift && (vk == 0x41 || vk == VK_SPACE.0 as u32))
        {
            let mut ctx = ENGINE.lock().unwrap();
            ctx.state.enabled = !ctx.state.enabled;
            ctx.buffer.clear();
            ctx.word_raw.clear();
            ctx.prev_was_consonant = false;
            ctx.last_unicode_char.clear();
            if let Some(app) = &ctx.app_handle {
                let _ = app.emit("ime-state-changed", ctx.state.clone());
            }
            return LRESULT(1);
        }

        if ctrl || alt {
            return unsafe { CallNextHookEx(get_hook_handle(), code, wparam, lparam) };
        }

        let mut ctx = ENGINE.lock().unwrap();
        if ctx.state.enabled {
            if vk == VK_SPACE.0 as u32 || vk == VK_BACK.0 as u32 || vk == 0x0D || vk == 0x09 {
                ctx.buffer.clear();
                ctx.word_raw.clear();
                ctx.prev_was_consonant = false;
                ctx.last_unicode_char.clear();
                drop(ctx);
                return unsafe { CallNextHookEx(get_hook_handle(), code, wparam, lparam) };
            }

            if let Some(ch) = vk_to_char(vk, shift) {
                let is_legacy = ctx.state.encoding == "legacy";
                ctx.word_raw.push(ch);

                match ctx.state.layout.as_str() {
                    "phonetic" => {
                        if let Some((uni_out, is_reph)) = translate_chart_phonetic(ch) {
                            if is_reph && !ctx.last_unicode_char.is_empty() && !is_legacy {
                                let combined = format!("ৰ্{}", ctx.last_unicode_char);
                                send_text(&combined, 1);
                                ctx.last_unicode_char = combined;
                            } else if is_legacy
                                && (uni_out == "ি" || uni_out == "ে" || uni_out == "ৈ")
                                && !ctx.last_unicode_char.is_empty()
                            {
                                let matra_ascii = unicode_to_legacy_ascii(uni_out);
                                let prev_cons_ascii =
                                    unicode_to_legacy_ascii(&ctx.last_unicode_char);
                                send_text(&format!("{}{}", matra_ascii, prev_cons_ascii), 1);
                                ctx.last_unicode_char = uni_out.to_string();
                            } else {
                                let final_out = if is_legacy {
                                    unicode_to_legacy_ascii(uni_out)
                                } else {
                                    uni_out.to_string()
                                };
                                send_text(&final_out, 0);
                                ctx.last_unicode_char = uni_out.to_string();
                            }
                            return LRESULT(1);
                        }
                    }
                    "ai_mode" => {
                        let combo = format!("{}{}", ctx.buffer, ch);
                        if let Some((uni_out, backspaces, is_cons)) =
                            translate_ai_mode(&ctx.word_raw, &combo, ch, ctx.prev_was_consonant)
                        {
                            if backspaces > 0 {
                                ctx.buffer.clear();
                            } else {
                                ctx.buffer = ch.to_string();
                            }
                            ctx.prev_was_consonant = is_cons;

                            if !uni_out.is_empty() {
                                if is_legacy
                                    && (uni_out == "ি" || uni_out == "ে" || uni_out == "ৈ")
                                    && !ctx.last_unicode_char.is_empty()
                                {
                                    let matra_ascii = unicode_to_legacy_ascii(&uni_out);
                                    let prev_cons_ascii =
                                        unicode_to_legacy_ascii(&ctx.last_unicode_char);
                                    send_text(&format!("{}{}", matra_ascii, prev_cons_ascii), 1);
                                } else {
                                    let final_out = if is_legacy {
                                        unicode_to_legacy_ascii(&uni_out)
                                    } else {
                                        uni_out.clone()
                                    };
                                    send_text(&final_out, backspaces);
                                }
                                ctx.last_unicode_char = uni_out;
                            }
                            return LRESULT(1);
                        }
                    }
                    _ => {
                        let combo = format!("{}{}", ctx.buffer, ch);
                        if let Some((uni_out, backspaces, is_cons)) =
                            translate_smart_assamese(&combo, ch, ctx.prev_was_consonant)
                        {
                            if backspaces > 0 {
                                ctx.buffer.clear();
                            } else {
                                ctx.buffer = ch.to_string();
                            }
                            ctx.prev_was_consonant = is_cons;

                            if !uni_out.is_empty() {
                                if is_legacy
                                    && (uni_out == "ি" || uni_out == "ে" || uni_out == "ৈ")
                                    && !ctx.last_unicode_char.is_empty()
                                {
                                    let matra_ascii = unicode_to_legacy_ascii(&uni_out);
                                    let prev_cons_ascii =
                                        unicode_to_legacy_ascii(&ctx.last_unicode_char);
                                    send_text(&format!("{}{}", matra_ascii, prev_cons_ascii), 1);
                                } else {
                                    let final_out = if is_legacy {
                                        unicode_to_legacy_ascii(&uni_out)
                                    } else {
                                        uni_out.clone()
                                    };
                                    send_text(&final_out, backspaces);
                                }
                                ctx.last_unicode_char = uni_out;
                            }
                            return LRESULT(1);
                        }
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(get_hook_handle(), code, wparam, lparam) }
}

#[tauri::command]
fn get_ime_state() -> ImeState {
    ENGINE.lock().unwrap().state.clone()
}

#[tauri::command]
fn update_ime_state(app: AppHandle, enabled: bool, layout: String, encoding: String) -> ImeState {
    let mut ctx = ENGINE.lock().unwrap();
    ctx.state.enabled = enabled;
    ctx.state.layout = layout;
    ctx.state.encoding = encoding;
    ctx.buffer.clear();
    ctx.word_raw.clear();
    ctx.prev_was_consonant = false;
    ctx.last_unicode_char.clear();
    let _ = app.emit("ime-state-changed", ctx.state.clone());
    ctx.state.clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            {
                let mut ctx = ENGINE.lock().unwrap();
                ctx.app_handle = Some(app.handle().clone());
            }

            std::thread::spawn(|| unsafe {
                let h_instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
                if let Ok(hook) =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), h_instance, 0)
                {
                    HOOK_HANDLE_RAW.store(hook.0 as isize, Ordering::SeqCst);
                }

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_ime_state, update_ime_state])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
