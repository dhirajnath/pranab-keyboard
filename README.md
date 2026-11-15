# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
# AsomIME — System-Wide Assamese Input Method Editor for Windows

**AsomIME** is a lightweight, zero-lag system-wide Assamese typing utility for Windows built with **Tauri v2**, **Rust**, **React**, and **Tailwind CSS**. Inspired by classic Assamese DTP tools like Ramdhenu[cite: 1], it hooks into the Windows input pipeline at the OS level so you can type seamlessly in Assamese inside any desktop application—including **MS Word, CorelDRAW, Adobe Photoshop, browsers, and code editors**.

---

## Key Features

* **System-Wide OS Keyboard Hook:** Uses native Windows Win32 APIs (`WH_KEYBOARD_LL` and `SendInput`) in Rust to intercept keystrokes globally and inject Assamese text directly into the active window.
* **Three Typing Modes:**
  1. **Phonetic:** Maps Normal and Shift keys according to the standard Assamese Phonetic keymap chart (including the `d` Link/*Hasanta* key, `z` *Ra-phala*, `Shift + Z` automatic *Reph* placement, and direct conjunct keys like `ক্ষ`, `জ্ঞ`, `ত্ৰ`, and `হ্ম`)[cite: 2].
  2. **Smart Assamese:** Intuitive type-as-you-spell Romanized transliteration (`k` → `ক`, `kh` → `খ`) with automatic *Juktakshar* (conjunct) chaining (`s` + `k` → `স্ক`) and inherent vowel sealing (`o`).
  3. **AI Mode:** Context-aware hybrid engine that dynamically resolves extended consonant clusters (`ksh`, `jny`, `shr`, `ngg`), chart conjunct shortcuts, and mixed-script punctuation automatically.
* **Dual Encoding Output (`Unicode` & `Legacy DTP`):**
  * **Unicode:** Standard UTF-16 Assamese output for modern web browsers, MS Office, and Windows apps.
  * **Legacy DTP:** Real-time conversion to legacy ASCII glyph codes with automatic pre-base *matra* reordering (`ি`, `ে`, `ৈ`) for traditional non-Unicode DTP fonts (*Geetanjali*, *Asomiya*, *Borluit*) in CorelDRAW and Photoshop[cite: 1].
* **Always-On-Top Floating Toolbar:** Compact, draggable pill-shaped dock bar synchronized in real time with global hotkeys.

---

## Global Shortcuts

| Shortcut | Action |
| :--- | :--- |
| **`Ctrl + Space`** | Toggle Assamese ↔ English typing globally |
| **`Ctrl + Alt + Shift + A`** | Alternate toggle shortcut (Master Keyboard standard)[cite: 2] |
| **`Ctrl + Alt + Shift + Space`** | Alternate toggle shortcut[cite: 2] |

---

## Tech Stack

* **Backend / OS Hook:** Rust, Tauri v2, `windows` crate (`Win32_UI_WindowsAndMessaging`, `Win32_UI_Input_KeyboardAndMouse`), `once_cell`
* **Frontend / Floating UI:** React, TypeScript, Vite, Tailwind CSS

---

## Getting Started (Development)

### Prerequisites
* [Node.js](https://nodejs.org/) (LTS)
* [Rust & Cargo](https://www.rust-lang.org/tools/install)
* Windows 10 or Windows 11 (WebView2 runtime)

### Run Locally
1. Clone the repository:
   ```bash
   git clone [https://github.com/](https://github.com/)<your-username>/assamese-ime.git
   cd assamese-ime
