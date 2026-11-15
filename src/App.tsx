import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import "./App.css";

interface ImeState {
  enabled: boolean;
  layout: string;
  encoding: string;
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

export default function App() {
  const [state, setState] = useState<ImeState>({
    enabled: false,
    layout: "phonetic",
    encoding: "unicode",
  });
  const [showChart, setShowChart] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let receivedEvent = false;
    let unlisten: (() => void) | undefined;

    const initialize = async () => {
      try {
        unlisten = await listen<ImeState>("ime-state-changed", (event) => {
          receivedEvent = true;
          if (!cancelled) {
            setState(event.payload);
          }
        });
        if (cancelled) {
          unlisten();
          return;
        }

        const currentState = await invoke<ImeState>("get_ime_state");
        if (!cancelled && !receivedEvent) {
          setState(currentState);
        }
      } catch (error) {
        console.error("Failed to initialize IME state:", error);
      }
    };

    void initialize();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const syncState = async (next: Partial<ImeState>) => {
    const updated = { ...state, ...next };
    try {
      const currentState = await invoke<ImeState>(
        "update_ime_state",
        updated,
      );
      setState(currentState);
    } catch (error) {
      console.error("Failed to update IME state:", error);
    }
  };

  const toggleChart = async () => {
    const nextOpen = !showChart;
    const win = getCurrentWindow();
    try {
      if (nextOpen) {
        await win.setSize(new LogicalSize(560, 325));
      } else {
        await win.setSize(new LogicalSize(460, 46));
      }
      setShowChart(nextOpen);
    } catch (error) {
      console.error("Failed to resize the keymap drawer:", error);
    }
  };

  const handleVirtualClick = async (charStr: string) => {
    try {
      await invoke("inject_character", { unicodeText: charStr });
    } catch (error) {
      console.error("Failed to insert the selected character:", error);
    }
  };

  return (
    <div className="h-screen w-screen flex flex-col select-none bg-transparent">
      {/* Main Slim Floating Pill Bar */}
      <div
        data-tauri-drag-region
        className="h-11.5 w-full flex items-center justify-between px-3 bg-slate-900/95 text-white border border-slate-700/80 rounded-full shadow-xl cursor-move shrink-0"
      >
        {/* Brand Badge */}
        <div data-tauri-drag-region className="flex items-center gap-2">
          <span className="w-6 h-6 rounded-md bg-amber-500 text-slate-950 font-bold flex items-center justify-center text-sm shadow">
            অ
          </span>
          <span
            data-tauri-drag-region
            className="text-xs font-semibold tracking-wide text-slate-200"
          >
            AsomIME
          </span>
        </div>

        {/* Language Toggle Button (Synced with Ctrl + Space) */}
        <button
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => syncState({ enabled: !state.enabled })}
          title="Shortcut: Ctrl + Space"
          className={`px-3 py-1 rounded-full text-xs font-bold transition cursor-pointer ${
            state.enabled
              ? "bg-emerald-500 text-slate-950 shadow-sm shadow-emerald-500/40"
              : "bg-slate-800 text-slate-300 hover:bg-slate-700"
          }`}
        >
          {state.enabled ? "অসমীয়া (ON)" : "ENG (OFF)"}
        </button>

        {/* Layout Selector */}
        <select
          value={state.layout}
          onChange={(e) => syncState({ layout: e.target.value })}
          className="bg-slate-800 text-slate-200 text-xs rounded-md px-2 py-1 border border-slate-700 outline-none cursor-pointer"
        >
          <option value="phonetic">Phonetic</option>
          <option value="smart_assamese">Smart Assamese</option>
          <option value="ai_mode">AI Mode</option>
        </select>

        {/* Encoding Mode */}
        <select
          value={state.encoding}
          onChange={(e) => syncState({ encoding: e.target.value })}
          className="bg-slate-800 text-slate-200 text-xs rounded-md px-2 py-1 border border-slate-700 outline-none cursor-pointer"
        >
          <option value="unicode">Unicode</option>
          <option value="legacy">Legacy DTP</option>
        </select>

        {/* Keymap & Juktakshar Cheat-Sheet Toggle */}
        <button
          onMouseDown={(e) => e.preventDefault()}
          onClick={toggleChart}
          title="Keymap & Juktakshar Cheat-Sheet"
          className={`w-6 h-6 rounded-full text-xs font-bold flex items-center justify-center transition cursor-pointer ${
            showChart
              ? "bg-amber-500 text-slate-950"
              : "bg-slate-800 text-slate-300 hover:bg-slate-700"
          }`}
        >
          ?
        </button>

        {/* Close App Button */}
        <button
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => getCurrentWindow().close()}
          title="Exit Assamese IME"
          className="text-slate-400 hover:text-rose-400 text-xs font-bold px-1.5 cursor-pointer"
        >
          ✕
        </button>
      </div>

      {/* Expandable Interactive Keymap & Juktakshar Drawer */}
      {showChart && (
        <div
          onMouseDown={(e) => e.preventDefault()}
          className="mt-1.5 flex-1 bg-slate-900/95 text-white border border-slate-700/80 rounded-2xl p-2.5 shadow-2xl flex flex-col justify-between overflow-hidden"
        >
          {/* Visual Phonetic Keyboard Grid */}
          <div className="space-y-1">
            <div className="flex justify-between items-center text-[10px] text-slate-400 px-1">
              <span>Phonetic Keymap (Click any glyph to insert)</span>
              <span>
                <span className="text-amber-400 font-semibold">Top</span>: Shift
                &nbsp;|&nbsp;
                <span className="text-emerald-400 font-semibold">Bottom</span>: Normal
              </span>
            </div>

            {KEY_ROWS.map((row, rIdx) => (
              <div key={rIdx} className="flex justify-center gap-1">
                {row.map((k) => (
                  <div
                    key={k.en}
                    className="flex flex-col items-center bg-slate-800/90 border border-slate-700/80 rounded px-1.5 py-0.5 min-w-9 hover:border-amber-500/70 transition"
                  >
                    <div className="w-full flex justify-between items-center text-[9px] leading-none">
                      <span className="text-slate-400 font-mono">{k.en}</span>
                      <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => handleVirtualClick(k.shift)}
                        className="text-amber-400 font-bold hover:scale-125 transition cursor-pointer"
                        title={`Shift + ${k.en} → ${k.shift}`}
                      >
                        {k.shift}
                      </button>
                    </div>
                    <button
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => handleVirtualClick(k.normal)}
                      className="text-xs text-emerald-300 font-bold leading-tight hover:scale-125 transition cursor-pointer"
                      title={`${k.en} → ${k.normal}`}
                    >
                      {k.normal === "্" ? "Link" : k.normal}
                    </button>
                  </div>
                ))}
              </div>
            ))}
          </div>

          {/* One-Click Juktakshar Bar */}
          <div className="pt-1.5 border-t border-slate-800">
            <div className="text-[10px] text-slate-400 mb-1 px-1">
              Quick Juktakshar (Conjuncts)
            </div>
            <div className="flex flex-wrap gap-1 justify-center">
              {JUKTAKSHARS.map((juk) => (
                <button
                  key={juk}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => handleVirtualClick(juk)}
                  className="px-2 py-0.5 bg-slate-800 hover:bg-emerald-600 hover:text-white text-slate-200 rounded text-xs font-semibold border border-slate-700 transition cursor-pointer"
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