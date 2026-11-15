import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import "./App.css";

interface ImeState {
  enabled: boolean;
  layout: string; // "phonetic" | "smart"
  active_font_id: string;
  active_app: string;
}

interface DetectedFontProfile {
  id: string;
  family_name: string;
  badge: string;
}

interface KeyDef {
  en: string;
  normal: string;
  shift: string;
}

const KEY_ROWS: KeyDef[][] = [
  [
    { en: "`", normal: "ং", shift: "‘" },
    { en: "1", normal: "১", shift: "হ্ম" },
    { en: "2", normal: "২", shift: "’" },
    { en: "3", normal: "৩", shift: ":" },
    { en: "4", normal: "৪", shift: "/" },
    { en: "5", normal: "৫", shift: ";" },
    { en: "6", normal: "৬", shift: "ত্ৰ" },
    { en: "7", normal: "৭", shift: "ক্ষ" },
    { en: "8", normal: "৮", shift: "জ্ঞ" },
    { en: "9", normal: "৯", shift: "(" },
    { en: "0", normal: "০", shift: ")" },
    { en: "-", normal: "-", shift: "ঃ" },
    { en: "=", normal: "ৃ", shift: "ঋ" },
  ],
  [
    { en: "Q", normal: "ৌ", shift: "ঔ" },
    { en: "W", normal: "ৈ", shift: "ঐ" },
    { en: "E", normal: "া", shift: "আ" },
    { en: "R", normal: "ী", shift: "ঈ" },
    { en: "T", normal: "ূ", shift: "ঊ" },
    { en: "Y", normal: "ব", shift: "ভ" },
    { en: "U", normal: "হ", shift: "ঙ" },
    { en: "I", normal: "গ", shift: "ঘ" },
    { en: "O", normal: "দ", shift: "ধ" },
    { en: "P", normal: "জ", shift: "ঝ" },
    { en: "[", normal: "ড", shift: "ঢ" },
    { en: "]", normal: "়", shift: "ঞ" },
    { en: "\\", normal: "য", shift: "ৎ" },
  ],
  [
    { en: "A", normal: "ো", shift: "ও" },
    { en: "S", normal: "ে", shift: "এ" },
    { en: "D", normal: "্", shift: "অ" },
    { en: "F", normal: "ি", shift: "ই" },
    { en: "G", normal: "ু", shift: "উ" },
    { en: "H", normal: "প", shift: "ফ" },
    { en: "J", normal: "ৰ", shift: "ৰ" },
    { en: "K", normal: "ক", shift: "খ" },
    { en: "L", normal: "ত", shift: "থ" },
    { en: ";", normal: "চ", shift: "ছ" },
    { en: "'", normal: "ট", shift: "ঠ" },
  ],
  [
    { en: "Z", normal: "্ৰ", shift: "ৰ্" },
    { en: "X", normal: "ং", shift: "ঁ" },
    { en: "C", normal: "ম", shift: "ণ" },
    { en: "V", normal: "ন", shift: "V" },
    { en: "B", normal: "ৱ", shift: "B" },
    { en: "N", normal: "ল", shift: "ঙ্গ" },
    { en: "M", normal: "স", shift: "শ" },
    { en: ",", normal: ",", shift: "ষ" },
    { en: ".", normal: ".", shift: "।" },
    { en: "/", normal: "য়", shift: "?" },
  ],
];

const JUKTAKSHARS = [
  "ক্ষ",
  "জ্ঞ",
  "ত্ৰ",
  "শ্ৰ",
  "ঙ্গ",
  "ঙ্ক",
  "হ্ম",
  "ন্দ",
  "ন্ত",
  "স্প",
  "স্ত",
  "প্ৰ",
  "ক্ৰ",
  "ব্ধ",
  "দ্ব",
  "দ্ধ",
  "শ্ব",
  "ল্ল",
];

const LAYOUT_OPTIONS = [
  { id: "phonetic", label: "Phonetic", hint: "DTP / Ramdhenu Phonetic (k → ক, Shift+K → খ)" },
  { id: "smart", label: "Smart", hint: "Romanized Smart Typing (K → ক, Kh → খ)" },
];

export default function App() {
  const [state, setState] = useState<ImeState>({
    enabled: false,
    layout: "phonetic",
    active_font_id: "builtin_ramdhenu",
    active_app: "Desktop",
  });
  const [fonts, setFonts] = useState<DetectedFontProfile[]>([]);
  const [showChart, setShowChart] = useState(false);
  const [scanning, setScanning] = useState(false);

  useEffect(() => {
    invoke<ImeState>("get_ime_state").then(setState);
    invoke<DetectedFontProfile[]>("get_detected_fonts").then(setFonts);

    const unlisten = listen<ImeState>("ime-state-changed", (event) => {
      setState(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const syncState = async (next: Partial<ImeState>) => {
    const updated = { ...state, ...next };

    // Auto-pair default font family when switching between Phonetic (DTP) and Smart (Unicode)
    if (next.layout && !next.active_font_id) {
      if (next.layout === "phonetic" && state.active_font_id === "auto_unicode") {
        updated.active_font_id = "builtin_ramdhenu";
      } else if (next.layout === "smart" && state.active_font_id === "builtin_ramdhenu") {
        updated.active_font_id = "auto_unicode";
      }
    }

    setState(updated);
    await invoke("update_ime_state", {
      enabled: updated.enabled,
      layout: updated.layout,
      activeFontId: updated.active_font_id,
    });
  };

  const handleRescanFonts = async () => {
    setScanning(true);
    const updatedFonts = await invoke<DetectedFontProfile[]>("rescan_system_fonts");
    setFonts(updatedFonts);
    setTimeout(() => setScanning(false), 300);
  };

  const toggleChart = async () => {
    const nextOpen = !showChart;
    setShowChart(nextOpen);
    const win = getCurrentWindow();
    if (nextOpen) {
      await win.setSize(new LogicalSize(600, 355));
    } else {
      await win.setSize(new LogicalSize(560, 48));
    }
  };

  const handleVirtualClick = async (charStr: string) => {
    await invoke("inject_character", { unicodeText: charStr });
  };

  const activeFontObj = fonts.find((f) => f.id === state.active_font_id);

  return (
    <div className="h-screen w-screen flex flex-col select-none bg-transparent p-0.5">
      {/* Main Frosted Glass Floating Pill Bar */}
      <div
        data-tauri-drag-region
        className="h-11 w-full flex items-center justify-between px-2.5 glass-shell text-zinc-100 border border-white/10 rounded-full cursor-move shrink-0"
      >
        {/* Drag Grip + App Emblem */}
        <div data-tauri-drag-region className="flex items-center gap-1.5 pl-1">
          <span
            data-tauri-drag-region
            className="text-zinc-500 text-xs tracking-tighter leading-none"
            title="Drag toolbar"
          >
            ⋮⋮
          </span>
          <div
            data-tauri-drag-region
            className="w-6 h-6 rounded-lg bg-linear-to-br from-orange-500 to-amber-600 text-white font-bold flex items-center justify-center text-sm shadow-inner border border-orange-300/30"
          >
            অ
          </div>
        </div>

        {/* Active Language Toggle Pill (Synced with Ctrl + Space) */}
        <button
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => syncState({ enabled: !state.enabled })}
          title="Toggle Assamese / English (Ctrl + Space)"
          className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold transition-all duration-150 cursor-pointer ${
            state.enabled
              ? "bg-emerald-500 text-zinc-950 emerald-glow"
              : "bg-zinc-800/90 text-zinc-300 hover:bg-zinc-700/80 border border-white/5"
          }`}
        >
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              state.enabled ? "bg-zinc-950 animate-pulse" : "bg-zinc-500"
            }`}
          />
          {state.enabled ? "অসমীয়া" : "ENG"}
        </button>

        {/* Segmented Typing Mode Switcher (Phonetic vs Smart) */}
        <div
          onMouseDown={(e) => e.preventDefault()}
          className="flex items-center bg-zinc-900/90 p-0.5 rounded-full border border-white/10"
        >
          {LAYOUT_OPTIONS.map((opt) => {
            const active = state.layout === opt.id;
            return (
              <button
                key={opt.id}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => syncState({ layout: opt.id })}
                title={opt.hint}
                className={`px-2.5 py-0.5 rounded-full text-xs font-medium transition-all duration-150 cursor-pointer ${
                  active
                    ? "bg-linear-to-r from-orange-500 to-amber-500 text-zinc-950 font-bold amber-glow"
                    : "text-zinc-400 hover:text-zinc-200"
                }`}
              >
                {opt.label}
              </button>
            );
          })}
        </div>

        {/* Installed Font Selector + Rescan */}
        <div className="flex items-center gap-1 bg-zinc-900/90 pl-2 pr-1 py-0.5 rounded-full border border-white/10">
          <select
            value={state.active_font_id}
            onChange={(e) => syncState({ active_font_id: e.target.value })}
            title="Select installed Assamese font style"
            className="bg-transparent text-zinc-200 text-xs font-medium max-w-40 truncate outline-none cursor-pointer"
          >
            {fonts.map((f) => (
              <option key={f.id} value={f.id} className="bg-zinc-900 text-zinc-100">
                [{f.badge}] {f.family_name}
              </option>
            ))}
          </select>

          <button
            onMouseDown={(e) => e.preventDefault()}
            onClick={handleRescanFonts}
            title="Scan Windows Fonts folder for newly installed fonts"
            className={`w-5 h-5 rounded-full flex items-center justify-center text-xs text-amber-400 hover:bg-white/10 transition cursor-pointer ${
              scanning ? "animate-spin" : ""
            }`}
          >
            ↻
          </button>
        </div>

        {/* Keymap Drawer & Close Controls */}
        <div className="flex items-center gap-1 pr-0.5">
          <button
            onMouseDown={(e) => e.preventDefault()}
            onClick={toggleChart}
            title="Keymap & Juktakshar Cheat-Sheet"
            className={`w-6 h-6 rounded-full text-xs font-bold flex items-center justify-center transition-all cursor-pointer ${
              showChart
                ? "bg-linear-to-br from-orange-500 to-amber-500 text-zinc-950 amber-glow"
                : "bg-zinc-800/90 text-zinc-300 hover:bg-zinc-700 border border-white/5"
            }`}
          >
            ?
          </button>

          <button
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => getCurrentWindow().close()}
            title="Exit AsomIME"
            className="w-6 h-6 rounded-full flex items-center justify-center text-zinc-400 hover:text-rose-400 hover:bg-rose-500/10 text-xs transition cursor-pointer"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Expandable Frosted Glass Keymap & Juktakshar Drawer */}
      {showChart && (
        <div
          onMouseDown={(e) => e.preventDefault()}
          className="mt-1.5 flex-1 glass-shell text-zinc-100 border border-white/10 rounded-2xl p-3 flex flex-col justify-between overflow-hidden"
        >
          {/* Header Legend */}
          <div className="flex justify-between items-center text-xs text-zinc-400 px-1 pb-1 border-b border-white/5">
            <span className="font-medium text-zinc-300">
              {state.layout === "phonetic"
                ? "Phonetic: k → ক, Shift+K → খ"
                : "Smart: K → ক, Kh → খ"}{" "}
              <span className="text-amber-400">
                ({activeFontObj?.family_name ?? "Unicode"})
              </span>
            </span>
            <div className="flex items-center gap-3">
              <span>
                <span className="text-amber-400 font-semibold">Top-Right</span>:
                Shift
              </span>
              <span>
                <span className="text-emerald-400 font-semibold">Center</span>:
                Normal
              </span>
              <span className="text-xs px-1.5 py-0.5 rounded bg-white/5 text-zinc-400">
                Ctrl + Space
              </span>
            </div>
          </div>

          {/* Visual Keyboard Rows */}
          <div className="space-y-1 py-1">
            {KEY_ROWS.map((row, rIdx) => (
              <div key={rIdx} className="flex justify-center gap-1">
                {row.map((k) => (
                  <div
                    key={k.en}
                    className="group flex flex-col justify-between bg-zinc-900/90 hover:bg-zinc-800/95 border border-white/10 hover:border-orange-500/50 rounded-lg px-1.5 py-1 min-w-9 transition-all"
                  >
                    <div className="w-full flex justify-between items-center text-xs leading-none">
                      <span className="text-zinc-500 font-mono">{k.en}</span>
                      <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => handleVirtualClick(k.shift)}
                        className="text-amber-400 hover:text-amber-300 font-bold hover:scale-125 transition cursor-pointer"
                        title={`Shift + ${k.en} → ${k.shift}`}
                      >
                        {k.shift}
                      </button>
                    </div>
                    <button
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => handleVirtualClick(k.normal)}
                      className="text-xs text-emerald-300 hover:text-emerald-200 font-bold leading-tight mt-0.5 hover:scale-110 transition cursor-pointer"
                      title={`${k.en} → ${k.normal}`}
                    >
                      {k.normal === "্" ? "Link" : k.normal}
                    </button>
                  </div>
                ))}
              </div>
            ))}
          </div>

          {/* Quick Juktakshar (Conjunct) Strip */}
          <div className="pt-1.5 border-t border-white/10">
            <div className="flex justify-between items-center mb-1 px-1">
              <span className="text-xs font-medium uppercase tracking-wider text-zinc-400">
                Quick Juktakshar (Conjuncts)
              </span>
              <span className="text-xs text-amber-400/90">
                App: {state.active_app}
              </span>
            </div>
            <div className="flex flex-wrap gap-1 justify-center">
              {JUKTAKSHARS.map((juk) => (
                <button
                  key={juk}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => handleVirtualClick(juk)}
                  className="px-2 py-0.5 bg-zinc-900 hover:bg-linear-to-r hover:from-orange-500 hover:to-amber-500 hover:text-zinc-950 text-zinc-200 rounded-md text-xs font-semibold border border-white/10 hover:border-transparent transition-all cursor-pointer"
                >
                  {juk}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}