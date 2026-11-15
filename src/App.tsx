import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

interface ImeState {
  enabled: boolean;
  layout: string;
  encoding: string;
}

export default function App() {
  const [state, setState] = useState<ImeState>({
    enabled: false,
    layout: "phonetic",
    encoding: "unicode",
  });

  useEffect(() => {
    invoke<ImeState>("get_ime_state").then(setState);

    const unlisten = listen<ImeState>("ime-state-changed", (event) => {
      setState(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const syncState = async (next: Partial<ImeState>) => {
    const updated = { ...state, ...next };
    setState(updated);
    await invoke("update_ime_state", updated);
  };

  return (
    <div
      data-tauri-drag-region
      className="h-screen w-screen flex items-center justify-between px-3 bg-slate-900/95 text-white border border-slate-700/80 rounded-full shadow-xl select-none cursor-move"
    >
      {/* Brand Badge */}
      <div data-tauri-drag-region className="flex items-center gap-2">
        <span className="w-6 h-6 rounded-md bg-amber-500 text-slate-950 font-bold flex items-center justify-center text-sm shadow">
          অ
        </span>
        <span data-tauri-drag-region className="text-xs font-semibold tracking-wide text-slate-200">
          AsomIME
        </span>
      </div>

      {/* Language Toggle Button (Synced with Ctrl + Space) */}
      <button
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

      {/* Close App Button */}
      <button
        onClick={() => getCurrentWindow().close()}
        title="Exit Assamese IME"
        className="text-slate-400 hover:text-rose-400 text-xs font-bold px-1.5 cursor-pointer"
      >
        ✕
      </button>
    </div>
  );
}