# AsomIME

AsomIME is a Windows Assamese input method with a floating toolbar. It uses
Tauri v2, Rust, React, TypeScript, and Tailwind CSS. Its keyboard processing
and text injection are implemented locally with Windows APIs; no network
service is used for typing.

## Features

- System-wide keyboard hook with `Ctrl + Space` to toggle Assamese input.
- Phonetic, Smart Assamese, and AI Mode layouts. AI Mode uses local
  transliteration rules and does not call an AI service.
- Unicode output and a legacy DTP character mapping for supported characters.
- Always-on-top floating toolbar for toggling the input state and selecting a
  layout or encoding.

The legacy mapping and key layouts are application-specific. Compatibility
with legacy fonts and third-party applications may vary.

## Requirements

- Windows 10 or 11
- Node.js LTS
- Rust and Cargo
- Microsoft C++ Build Tools and the WebView2 Runtime (required by Tauri)

See the [Tauri Windows prerequisites](https://tauri.app/start/prerequisites/#windows)
for setup details.

## Development

```powershell
git clone https://github.com/dhirajnath/assamese-ime.git
cd assamese-ime
npm install
npm run tauri dev
```

To build the frontend or package the Windows application:

```powershell
npm run build
npm run tauri build
```

## Global shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl + Space` | Toggle Assamese input |
| `Ctrl + Alt + Shift + A` | Alternate toggle |
| `Ctrl + Alt + Shift + Space` | Alternate toggle |
