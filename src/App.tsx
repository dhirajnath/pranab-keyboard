import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import {
  Keyboard,
  RefreshCw,
  LayoutGrid,
  GripHorizontal,
  X,
  Sparkles,
  Type,
} from "lucide-react";

interface ImeState {
  enabled: boolean;
  layout: string; // "phonetic" | "smart"
  active_font_id: string;
  active_app: string;
}

interface ConvertedFontInfo {
  id: string;
  original_name: string;
  unicode_family_name: string;
  script_label: string;
  output_path: string;
  newly_converted: boolean;
}

const CONJUNCTS = [
  { label: "ক্ষ", seq: "ক্ষ", hint: "k+d+< / ksh" },
  { label: "জ্ঞ", seq: "জ্ঞ", hint: "p+d+} / jny" },
  { label: "ত্ৰ", seq: "ত্ৰ", hint: "l+d+j / tr" },
  { label: "শ্ৰ", seq: "শ্ৰ", hint: "M+d+j / shr" },
  { label: "ঙ্গ", seq: "ঙ্গ", hint: "U+d+i / ngg" },
  { label: "ঙ্ক", seq: "ঙ্ক", hint: "U+d+k / ngk" },
  { label: "হ্ম", seq: "হ্ম", hint: "u+d+c / hm" },
  { label: "ন্দ", seq: "ন্দ", hint: "v+d+o / nd" },
  { label: "স্ত", seq: "স্ত", hint: "m+d+l / st" },
  { label: "প্ৰ", seq: "প্ৰ", hint: "h+d+j / pr" },
];

const PHONETIC_HINTS = [
  { key: "k / K", out: "ক / খ" },
  { key: "i / I", out: "গ / ঘ" },
  { key: "; / :", out: "চ / ছ" },
  { key: "p / P", out: "জ / ঝ" },
  { key: "' / \"", out: "ট / ঠ" },
  { key: "[ / {", out: "ড / ঢ" },
  { key: "l / L", out: "ত / থ" },
  { key: "o / O", out: "দ / ধ" },
  { key: "v / C", out: "ন / ণ" },
  { key: "h / H", out: "প / ফ" },
  { key: "y / Y", out: "ব / ভ" },
  { key: "c / j", out: "ম / ৰ" },
  { key: "n / b", out: "ল / ৱ" },
  { key: "M / < / m", out: "শ / ষ / স" },
  { key: "u / & / *", out: "হ / ক্ষ / জ্ঞ" },
  { key: "e / f / r", out: "া / ি / ী" },
  { key: "g / t / =", out: "ু / ূ / ৃ" },
  { key: "s / w / a / q", out: "ে / ৈ / ো / ৌ" },
];

const WINDOW_WIDTH = 550;

function PranabAppIcon() {
  return (
    <div
      data-tauri-drag-region
      title="Pranab Keyboard"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: "24px",
        height: "24px",
        borderRadius: "7px",
        background: "linear-gradient(135deg, #f59e0b 0%, #d97706 100%)",
        boxShadow: "0 2px 6px rgba(245, 158, 11, 0.35)",
        color: "#09090b",
        fontWeight: 800,
        fontSize: "14px",
        lineHeight: 1,
        flexShrink: 0,
      }}
    >
      <span style={{ pointerEvents: "none", marginTop: "-1px" }}>অ</span>
    </div>
  );
}

export default function App() {
  const [imeState, setImeState] = useState<ImeState>({
    enabled: false,
    layout: "phonetic",
    active_font_id: "auto_unicode",
    active_app: "Desktop",
  });
  const [fonts, setFonts] = useState<ConvertedFontInfo[]>([]);
  const [scanning, setScanning] = useState(false);
  const [showKeymap, setShowKeymap] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<ImeState>("get_ime_state").then(setImeState).catch(console.error);
    invoke<ConvertedFontInfo[]>("get_detected_fonts")
      .then(setFonts)
      .catch(console.error);

    const unlisten = listen<ImeState>("ime-state-changed", (event) => {
      setImeState(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const el = contentRef.current;
    if (!el) return;

    const syncWindowSize = () => {
      const measuredHeight = Math.ceil(el.getBoundingClientRect().height);
      if (measuredHeight > 0) {
        getCurrentWindow()
          .setSize(new LogicalSize(WINDOW_WIDTH, measuredHeight))
          .catch(console.error);
      }
    };

    syncWindowSize();

    const observer = new ResizeObserver(() => {
      syncWindowSize();
    });
    observer.observe(el);

    return () => observer.disconnect();
  }, [showKeymap]);

  const syncState = async (
    enabled: boolean,
    layout: string,
    active_font_id: string
  ) => {
    try {
      const updated = await invoke<ImeState>("update_ime_state", {
        enabled,
        layout,
        activeFontId: active_font_id,
      });
      setImeState(updated);
    } catch (err) {
      console.error(err);
    }
  };

  const handleToggleEnabled = () => {
    syncState(!imeState.enabled, imeState.layout, imeState.active_font_id);
  };

  const handleLayoutChange = (newLayout: string) => {
    syncState(imeState.enabled, newLayout, imeState.active_font_id);
  };

  const handleFontChange = (newFontId: string) => {
    syncState(imeState.enabled, imeState.layout, newFontId);
  };

  const handleRescan = async () => {
    setScanning(true);
    try {
      const updatedFonts = await invoke<ConvertedFontInfo[]>(
        "rescan_system_fonts"
      );
      setFonts(updatedFonts);
    } catch (err) {
      console.error(err);
    } finally {
      setScanning(false);
    }
  };

  const handleInject = async (text: string) => {
    try {
      await invoke("inject_character", { unicodeText: text });
    } catch (err) {
      console.error(err);
    }
  };

  const handleMinimize = async () => {
    await getCurrentWindow().minimize();
  };

  return (
    <div
      ref={contentRef}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        width: "100%",
        height: "fit-content",
        padding: "6px",
        boxSizing: "border-box",
        background: "transparent",
        color: "#f4f4f5",
      }}
    >
      {/* Floating Frosted Obsidian Toolbar */}
      <div
        data-tauri-drag-region
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: "8px",
          padding: "6px 12px",
          borderRadius: "9999px",
          backgroundColor: "rgba(9, 9, 11, 0.94)",
          border: "1px solid rgba(63, 63, 70, 0.85)",
          boxShadow: "0 8px 24px rgba(0, 0, 0, 0.65)",
          backdropFilter: "blur(16px)",
          whiteSpace: "nowrap",
        }}
      >
        {/* Drag Handle */}
        <div
          data-tauri-drag-region
          title="Drag Pranab Keyboard"
          style={{
            display: "flex",
            alignItems: "center",
            cursor: "grab",
            color: "#71717a",
          }}
        >
          <GripHorizontal className="w-4 h-4 pointer-events-none" />
        </div>

        {/* App Icon */}
        <PranabAppIcon />

        {/* Active Language Toggle Pill */}
        <button
          onClick={handleToggleEnabled}
          title="Toggle Language (Ctrl + Space)"
          style={{
            display: "flex",
            alignItems: "center",
            gap: "6px",
            padding: "4px 12px",
            borderRadius: "9999px",
            border: "none",
            cursor: "pointer",
            fontSize: "12px",
            fontWeight: 700,
            backgroundColor: imeState.enabled ? "#f59e0b" : "#27272a",
            color: imeState.enabled ? "#09090b" : "#e4e4e7",
            boxShadow: imeState.enabled
              ? "0 0 14px rgba(245, 158, 11, 0.4)"
              : "none",
            transition: "all 0.15s ease",
          }}
        >
          <Keyboard className="w-3.5 h-3.5" />
          <span>{imeState.enabled ? "অসমীয়া" : "ENG"}</span>
        </button>

        {/* Segmented Mode Switcher */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            backgroundColor: "#18181b",
            border: "1px solid #27272a",
            borderRadius: "9999px",
            padding: "2px",
          }}
        >
          <button
            onClick={() => handleLayoutChange("phonetic")}
            title="Master Keyboard DX Phonetic Layout"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "4px",
              padding: "3px 10px",
              borderRadius: "9999px",
              border: "none",
              cursor: "pointer",
              fontSize: "11px",
              fontWeight: 600,
              backgroundColor:
                imeState.layout === "phonetic" ? "#3f3f46" : "transparent",
              color: imeState.layout === "phonetic" ? "#fcd34d" : "#a1a1aa",
            }}
          >
            <Type className="w-3 h-3" />
            <span>Phonetic</span>
          </button>
          <button
            onClick={() => handleLayoutChange("smart")}
            title="Smart Romanized Transliteration"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "4px",
              padding: "3px 10px",
              borderRadius: "9999px",
              border: "none",
              cursor: "pointer",
              fontSize: "11px",
              fontWeight: 600,
              backgroundColor:
                imeState.layout === "smart" ? "#3f3f46" : "transparent",
              color: imeState.layout === "smart" ? "#fcd34d" : "#a1a1aa",
            }}
          >
            <Sparkles className="w-3 h-3" />
            <span>Smart</span>
          </button>
        </div>

        {/* Auto-Converted Unicode Font Selector */}
        <div style={{ display: "flex", alignItems: "center", gap: "4px" }}>
          <select
            value={imeState.active_font_id}
            onChange={(e) => handleFontChange(e.target.value)}
            title={`Active App: ${imeState.active_app}`}
            style={{
              backgroundColor: "#18181b",
              border: "1px solid #27272a",
              color: "#e4e4e7",
              fontSize: "11px",
              borderRadius: "9999px",
              padding: "4px 10px",
              maxWidth: "175px",
              outline: "none",
              cursor: "pointer",
            }}
          >
            {fonts.map((f) => (
              <option
                key={f.id}
                value={f.id}
                style={{ backgroundColor: "#18181b", color: "#f4f4f5" }}
              >
                [{f.script_label}] {f.unicode_family_name}
              </option>
            ))}
          </select>

          <button
            onClick={handleRescan}
            disabled={scanning}
            title="Scan & Auto-Convert Legacy Fonts to Unicode"
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "26px",
              height: "26px",
              borderRadius: "9999px",
              backgroundColor: "#18181b",
              border: "1px solid #27272a",
              color: scanning ? "#fbbf24" : "#a1a1aa",
              cursor: "pointer",
            }}
          >
            <RefreshCw
              className={`w-3.5 h-3.5 ${scanning ? "animate-spin" : ""}`}
            />
          </button>
        </div>

        {/* Keymap & Juktakshar Drawer Toggle */}
        <button
          onClick={() => setShowKeymap((prev) => !prev)}
          title={
            showKeymap
              ? "Hide Keymap & Juktakshar Chart"
              : "Show Keymap & Juktakshar Chart"
          }
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "26px",
            height: "26px",
            borderRadius: "9999px",
            backgroundColor: showKeymap
              ? "rgba(245, 158, 11, 0.25)"
              : "#18181b",
            border: showKeymap
              ? "1px solid rgba(245, 158, 11, 0.6)"
              : "1px solid #27272a",
            color: showKeymap ? "#fcd34d" : "#a1a1aa",
            cursor: "pointer",
            transition: "all 0.15s ease",
          }}
        >
          <LayoutGrid className="w-3.5 h-3.5" />
        </button>

        {/* Minimize Button */}
        <button
          onClick={handleMinimize}
          title="Minimize Toolbar"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "24px",
            height: "24px",
            borderRadius: "9999px",
            backgroundColor: "transparent",
            border: "none",
            color: "#71717a",
            cursor: "pointer",
          }}
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Expandable Keymap & Juktakshar Drawer */}
      {showKeymap && (
        <div
          style={{
            marginTop: "8px",
            width: "526px",
            maxWidth: "100%",
            borderRadius: "16px",
            backgroundColor: "rgba(9, 9, 11, 0.96)",
            border: "1px solid #27272a",
            padding: "12px",
            boxSizing: "border-box",
            boxShadow: "0 16px 40px rgba(0, 0, 0, 0.75)",
            backdropFilter: "blur(16px)",
            fontSize: "12px",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              paddingBottom: "8px",
              marginBottom: "8px",
              borderBottom: "1px solid #27272a",
            }}
          >
            <span style={{ fontWeight: 700, color: "#fbbf24" }}>
              Pranab Keyboard — Keymap & Juktakshar Chart
            </span>
            <span style={{ fontSize: "11px", color: "#a1a1aa" }}>
              App: <strong style={{ color: "#e4e4e7" }}>{imeState.active_app}</strong> · Shortcut:{" "}
              <kbd
                style={{
                  padding: "2px 6px",
                  backgroundColor: "#27272a",
                  borderRadius: "4px",
                  color: "#e4e4e7",
                }}
              >
                Ctrl+Space
              </kbd>
            </span>
          </div>

          {/* Clickable Conjuncts */}
          <div style={{ marginBottom: "10px" }}>
            <div
              style={{
                fontSize: "11px",
                color: "#a1a1aa",
                marginBottom: "6px",
              }}
            >
              Click to insert Juktakshar:
            </div>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(5, 1fr)",
                gap: "6px",
              }}
            >
              {CONJUNCTS.map((c) => (
                <button
                  key={c.label}
                  onClick={() => handleInject(c.seq)}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    padding: "4px 6px",
                    borderRadius: "8px",
                    backgroundColor: "#18181b",
                    border: "1px solid #27272a",
                    color: "#f4f4f5",
                    cursor: "pointer",
                  }}
                >
                  <span style={{ fontSize: "15px", fontWeight: 600 }}>
                    {c.label}
                  </span>
                  <span style={{ fontSize: "10px", color: "#a1a1aa" }}>
                    {c.hint}
                  </span>
                </button>
              ))}
            </div>
          </div>

          {/* Keymap Grid */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(3, 1fr)",
              gap: "6px",
              paddingTop: "8px",
              borderTop: "1px solid #27272a",
            }}
          >
            {PHONETIC_HINTS.map((item) => (
              <div
                key={item.key}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "4px 8px",
                  borderRadius: "6px",
                  backgroundColor: "#18181b",
                  border: "1px solid rgba(39, 39, 42, 0.7)",
                }}
              >
                <span
                  style={{
                    fontFamily: "monospace",
                    fontSize: "11px",
                    color: "#fcd34d",
                  }}
                >
                  {item.key}
                </span>
                <span style={{ fontWeight: 600, color: "#f4f4f5" }}>
                  {item.out}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}