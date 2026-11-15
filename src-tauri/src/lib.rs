mod font_engine;

use font_engine::{
    scan_installed_assamese_fonts, shape_unicode_for_profile, DetectedFontProfile, FontStandard,
};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_MENU, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

#[derive(Clone, Serialize)]
pub struct ImeState {
    pub enabled: bool,
    pub layout: String,         // "phonetic" | "smart"
    pub active_font_id: String, // e.g., "builtin_ramdhenu" or "auto_unicode"
    pub active_app: String,
}

struct EngineContext {
    state: ImeState,
    installed_fonts: Vec<DetectedFontProfile>,
    per_app_font_memory: HashMap<String, String>,
    buffer: String,
    word_unicode: String,
    word_emitted_len: usize,
    prev_was_consonant: bool,
    app_handle: Option<AppHandle>,
}

static ENGINE: Lazy<Mutex<EngineContext>> = Lazy::new(|| {
    let fonts = scan_installed_assamese_fonts();
    Mutex::new(EngineContext {
        state: ImeState {
            enabled: false,
            layout: "phonetic".into(),
            active_font_id: "builtin_ramdhenu".into(),
            active_app: "Desktop".into(),
        },
        installed_fonts: fonts,
        per_app_font_memory: HashMap::new(),
        buffer: String::new(),
        word_unicode: String::new(),
        word_emitted_len: 0,
        prev_was_consonant: false,
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

fn get_foreground_exe_name() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return "Desktop".into();
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return "Desktop".into();
        }
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 260];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);
            if ok && size > 0 {
                let full_path = String::from_utf16_lossy(&buf[..size as usize]);
                if let Some(exe) = full_path.rsplit('\\').next() {
                    return exe.to_string();
                }
            }
        }
        "Desktop".into()
    }
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

/// Smart Mode: Natural Romanized Transliteration
fn translate_smart_mode(
    combo: &str,
    single: char,
    after_consonant: bool,
) -> Option<(String, usize, bool)> {
    let lower_combo = combo.to_ascii_lowercase();

    match lower_combo.as_str() {
        "ksh" => return Some(("ক্ষ".to_string(), 2, true)),
        "jny" | "gya" => return Some(("জ্ঞ".to_string(), 2, true)),
        "shr" => return Some(("শ্ৰ".to_string(), 2, true)),
        "ngk" => return Some(("ঙ্ক".to_string(), 2, true)),
        "ngg" => return Some(("ঙ্গ".to_string(), 2, true)),
        _ => {}
    }

    let digraph = match lower_combo.as_str() {
        "kh" => Some(("খ", 1, true)),
        "gh" => Some(("ঘ", 1, true)),
        "ng" => Some(("ঙ", 1, true)),
        "ch" => Some(("ছ", 1, true)),
        "jh" => Some(("ঝ", 1, true)),
        "th" if combo.starts_with('T') => Some(("ঠ", 1, true)),
        "dh" if combo.starts_with('D') => Some(("ঢ", 1, true)),
        "th" => Some(("থ", 1, true)),
        "dh" => Some(("ধ", 1, true)),
        "ph" => Some(("ফ", 1, true)),
        "bh" => Some(("ভ", 1, true)),
        "sh" if combo.starts_with('S') => Some(("ষ", 1, true)),
        "sh" => Some(("শ", 1, true)),
        "rh" if combo.starts_with('R') => Some(("ঢ়", 1, true)),
        "hm" => Some(("হ্ম", 1, true)),
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
        'k' | 'K' => Some("ক"),
        'g' | 'G' => Some("গ"),
        'c' | 'C' => Some("চ"),
        'j' | 'J' => Some("জ"),
        'T' => Some("ট"),
        'D' => Some("ড"),
        'N' => Some("ণ"),
        't' => Some("ত"),
        'd' => Some("দ"),
        'n' => Some("ন"),
        'p' | 'P' => Some("প"),
        'f' | 'F' => Some("ফ"),
        'b' | 'B' => Some("ব"),
        'v' | 'V' => Some("ভ"),
        'm' | 'M' => Some("ম"),
        'z' | 'Z' => Some("য"),
        'r' => Some("ৰ"),
        'l' | 'L' => Some("ল"),
        'w' | 'W' => Some("ৱ"),
        's' | 'S' => Some("স"),
        'h' | 'H' => Some("হ"),
        'R' => Some("ড়"),
        'y' | 'Y' => Some("য়"),
        'x' | 'X' => Some("ক্ষ"),
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
        'a' | 'A' => Some(if after_consonant { "া" } else { "অ" }),
        'i' | 'I' => Some(if after_consonant { "ি" } else { "ই" }),
        'u' | 'U' => Some(if after_consonant { "ু" } else { "উ" }),
        'e' | 'E' => Some(if after_consonant { "ে" } else { "এ" }),
        'o' => Some("ও"),
        'O' => Some(if after_consonant { "ো" } else { "ও" }),
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

fn pop_unicode_chars(s: &mut String, count: usize) {
    for _ in 0..count {
        s.pop();
    }
}

fn reset_word_buffer(ctx: &mut EngineContext) {
    ctx.buffer.clear();
    ctx.word_unicode.clear();
    ctx.word_emitted_len = 0;
    ctx.prev_was_consonant = false;
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && !IS_INJECTING.load(Ordering::SeqCst)
        && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN)
    {
        let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let ctrl = is_key_down(VK_CONTROL);
        let alt = is_key_down(VK_MENU);
        let shift = is_key_down(VK_SHIFT);

        if (ctrl && vk == VK_SPACE.0 as u32)
            || (ctrl && alt && shift && (vk == 0x41 || vk == VK_SPACE.0 as u32))
        {
            let mut ctx = ENGINE.lock().unwrap();
            ctx.state.enabled = !ctx.state.enabled;
            reset_word_buffer(&mut ctx);
            if let Some(app) = &ctx.app_handle {
                let _ = app.emit("ime-state-changed", ctx.state.clone());
            }
            return LRESULT(1);
        }

        if ctrl || alt {
            return CallNextHookEx(get_hook_handle(), code, wparam, lparam);
        }

        let current_exe = get_foreground_exe_name();
        let mut ctx = ENGINE.lock().unwrap();

        if !current_exe.eq_ignore_ascii_case("assamese-ime.exe")
            && ctx.state.active_app != current_exe
        {
            ctx.state.active_app = current_exe.clone();
            reset_word_buffer(&mut ctx);
            if let Some(saved_font_id) = ctx.per_app_font_memory.get(&current_exe).cloned() {
                ctx.state.active_font_id = saved_font_id;
            }
            if let Some(app) = &ctx.app_handle {
                let _ = app.emit("ime-state-changed", ctx.state.clone());
            }
        }

        if ctx.state.enabled {
            if vk == VK_SPACE.0 as u32 || vk == VK_BACK.0 as u32 || vk == 0x0D || vk == 0x09 {
                reset_word_buffer(&mut ctx);
                return CallNextHookEx(get_hook_handle(), code, wparam, lparam);
            }

            if let Some(ch) = vk_to_char(vk, shift) {
                let active_profile = ctx
                    .installed_fonts
                    .iter()
                    .find(|f| f.id == ctx.state.active_font_id)
                    .cloned()
                    .unwrap_or_else(|| ctx.installed_fonts[0].clone());

                // DIRECT PHONETIC PASS-THROUGH FOR GEETANJALI / RAMDHENU:
                // Since Master Keyboard DX layout maps 1-to-1 directly to Geetanjali ASCII slots,
                // we pass through the exact typed character instantly without any wrapping or buffering.
                if ctx.state.layout == "phonetic"
                    && active_profile.standard == FontStandard::RamdhenuGeetanjali
                {
                    send_text(&ch.to_string(), 0);
                    return LRESULT(1);
                }

                // STANDARD SMART MODE / UNICODE SHAPING PATH:
                let mut translated = false;

                if ctx.state.layout == "phonetic" {
                    if let Some((uni_out, is_reph)) =
                        font_engine::translate_ramdhenu_phonetic_fallback(ch)
                    {
                        if is_reph && !ctx.word_unicode.is_empty() {
                            if let Some(last_c) = ctx.word_unicode.pop() {
                                ctx.word_unicode.push_str("ৰ্");
                                ctx.word_unicode.push(last_c);
                            }
                        } else {
                            ctx.word_unicode.push_str(uni_out);
                        }
                        translated = true;
                    }
                } else {
                    let combo = format!("{}{}", ctx.buffer, ch);
                    if let Some((uni_out, backspaces, is_cons)) =
                        translate_smart_mode(&combo, ch, ctx.prev_was_consonant)
                    {
                        if backspaces > 0 {
                            pop_unicode_chars(&mut ctx.word_unicode, backspaces);
                        }
                        let next_buf = format!("{}{}", ctx.buffer, ch);
                        ctx.buffer = next_buf
                            .chars()
                            .rev()
                            .take(2)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();

                        ctx.prev_was_consonant = is_cons;
                        ctx.word_unicode.push_str(&uni_out);
                        translated = true;
                    }
                }

                if translated {
                    let shaped_word = shape_unicode_for_profile(&ctx.word_unicode, &active_profile);
                    let prev_len = ctx.word_emitted_len;
                    ctx.word_emitted_len = shaped_word.chars().count();

                    send_text(&shaped_word, prev_len);
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(get_hook_handle(), code, wparam, lparam)
}

#[tauri::command]
fn get_ime_state() -> ImeState {
    ENGINE.lock().unwrap().state.clone()
}

#[tauri::command]
fn get_detected_fonts() -> Vec<DetectedFontProfile> {
    ENGINE.lock().unwrap().installed_fonts.clone()
}

#[tauri::command]
fn rescan_system_fonts() -> Vec<DetectedFontProfile> {
    let fonts = scan_installed_assamese_fonts();
    let mut ctx = ENGINE.lock().unwrap();
    ctx.installed_fonts = fonts.clone();
    fonts
}

#[tauri::command]
fn update_ime_state(
    app: AppHandle,
    enabled: bool,
    layout: String,
    active_font_id: String,
) -> ImeState {
    let mut ctx = ENGINE.lock().unwrap();
    ctx.state.enabled = enabled;
    ctx.state.layout = layout;
    ctx.state.active_font_id = active_font_id.clone();

    let current_app = ctx.state.active_app.clone();
    if current_app != "Desktop" && !current_app.eq_ignore_ascii_case("assamese-ime.exe") {
        ctx.per_app_font_memory.insert(current_app, active_font_id);
    }

    reset_word_buffer(&mut ctx);
    let _ = app.emit("ime-state-changed", ctx.state.clone());
    ctx.state.clone()
}

#[tauri::command]
fn inject_character(unicode_text: String) {
    let profile = {
        let ctx = ENGINE.lock().unwrap();
        ctx.installed_fonts
            .iter()
            .find(|f| f.id == ctx.state.active_font_id)
            .cloned()
            .unwrap_or_else(|| ctx.installed_fonts[0].clone())
    };
    let output = shape_unicode_for_profile(&unicode_text, &profile);
    send_text(&output, 0);
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
        .invoke_handler(tauri::generate_handler![
            get_ime_state,
            get_detected_fonts,
            rescan_system_fonts,
            update_ime_state,
            inject_character
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
