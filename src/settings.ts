// User settings: schema, persistence (settings.json in config dir), and live
// application to the CSS accent + xterm.js terminal options.
import type { ITerminalOptions, ITheme } from "@xterm/xterm";
import { api, type LocalShell } from "./ipc";

export interface Settings {
  accent: string;
  accent2: string;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  cursorStyle: "bar" | "block" | "underline";
  cursorBlink: boolean;
  scrollback: number;
  copyOnSelect: boolean;
  rightClick: "paste" | "menu";
  defaultShell: LocalShell;
  bell: "none" | "visual" | "sound";
}

export const DEFAULTS: Settings = {
  accent: "#7c5cff",
  accent2: "#22d3ee",
  fontFamily:
    '"Cascadia Code", "Cascadia Mono", "JetBrains Mono", "Consolas", ui-monospace, monospace',
  fontSize: 13.5,
  lineHeight: 1.2,
  cursorStyle: "bar",
  cursorBlink: true,
  scrollback: 10000,
  copyOnSelect: true,
  rightClick: "paste",
  defaultShell: "powershell",
  bell: "visual",
};

/** Live settings object. Mutated in place so importers see updates. */
export const current: Settings = { ...DEFAULTS };

export const ACCENT_PRESETS: Array<{ name: string; accent: string; accent2: string }> = [
  { name: "Indigo", accent: "#7c5cff", accent2: "#22d3ee" },
  { name: "Emerald", accent: "#10b981", accent2: "#84cc16" },
  { name: "Sunset", accent: "#fb7185", accent2: "#f59e0b" },
  { name: "Ocean", accent: "#3b82f6", accent2: "#06b6d4" },
  { name: "Violet", accent: "#a855f7", accent2: "#ec4899" },
  { name: "Amber", accent: "#f59e0b", accent2: "#eab308" },
];

const BASE_THEME: ITheme = {
  background: "#0e0f13",
  foreground: "#cdd2df",
  cursorAccent: "#0e0f13",
  selectionForeground: "#f2f4fa",
  black: "#12141c",
  red: "#ff6b6b",
  green: "#5ef2a0",
  yellow: "#ffd166",
  blue: "#5aa2ff",
  magenta: "#c792ff",
  cyan: "#22d3ee",
  white: "#cdd2df",
  brightBlack: "#5a6274",
  brightRed: "#ff8b8b",
  brightGreen: "#8ef7bd",
  brightYellow: "#ffe08a",
  brightBlue: "#89c2ff",
  brightMagenta: "#dcb4ff",
  brightCyan: "#7ee8f7",
  brightWhite: "#f2f4fa",
};

export function termTheme(): ITheme {
  return {
    ...BASE_THEME,
    cursor: current.accent,
    selectionBackground: hexA(current.accent, 0.34),
  };
}

export function termOptions(): ITerminalOptions {
  return {
    fontFamily: current.fontFamily,
    fontSize: current.fontSize,
    lineHeight: current.lineHeight,
    cursorStyle: current.cursorStyle,
    cursorBlink: current.cursorBlink,
    cursorWidth: 2,
    scrollback: current.scrollback,
    fontWeight: 400,
    fontWeightBold: 600,
    allowProposedApi: true,
    drawBoldTextInBrightColors: true,
    minimumContrastRatio: 1.1,
    macOptionIsMeta: true,
    theme: termTheme(),
  };
}

export function applyAccent(): void {
  const s = document.documentElement.style;
  s.setProperty("--accent", current.accent);
  s.setProperty("--accent-2", current.accent2);
  s.setProperty("--accent-soft", hexA(current.accent, 0.16));
  s.setProperty("--accent-ring", hexA(current.accent, 0.45));
}

export async function loadSettings(): Promise<void> {
  try {
    const raw = (await api.settingsLoad()) ?? {};
    Object.assign(current, DEFAULTS, sanitize(raw));
  } catch {
    Object.assign(current, DEFAULTS);
  }
  applyAccent();
}

export async function persistSettings(): Promise<void> {
  applyAccent();
  try {
    await api.settingsSave(current as unknown as Record<string, unknown>);
  } catch {
    /* non-fatal */
  }
}

function sanitize(raw: Record<string, unknown>): Partial<Settings> {
  const out: Partial<Settings> = {};
  const str = (v: unknown) => (typeof v === "string" ? v : undefined);
  const num = (v: unknown) => (typeof v === "number" && isFinite(v) ? v : undefined);
  const bool = (v: unknown) => (typeof v === "boolean" ? v : undefined);
  if (str(raw.accent)) out.accent = raw.accent as string;
  if (str(raw.accent2)) out.accent2 = raw.accent2 as string;
  if (str(raw.fontFamily)) out.fontFamily = raw.fontFamily as string;
  if (num(raw.fontSize)) out.fontSize = clamp(raw.fontSize as number, 8, 32);
  if (num(raw.lineHeight)) out.lineHeight = clamp(raw.lineHeight as number, 1, 2.5);
  if (raw.cursorStyle === "bar" || raw.cursorStyle === "block" || raw.cursorStyle === "underline")
    out.cursorStyle = raw.cursorStyle;
  if (bool(raw.cursorBlink) !== undefined) out.cursorBlink = raw.cursorBlink as boolean;
  if (num(raw.scrollback)) out.scrollback = clamp(raw.scrollback as number, 100, 200000);
  if (bool(raw.copyOnSelect) !== undefined) out.copyOnSelect = raw.copyOnSelect as boolean;
  if (raw.rightClick === "paste" || raw.rightClick === "menu") out.rightClick = raw.rightClick;
  if (["cmd", "powershell", "pwsh", "bash"].includes(raw.defaultShell as string))
    out.defaultShell = raw.defaultShell as LocalShell;
  if (raw.bell === "none" || raw.bell === "visual" || raw.bell === "sound") out.bell = raw.bell;
  return out;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, n));
}

function hexA(hex: string, a: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return `rgba(124,92,255,${a})`;
  const n = parseInt(m[1], 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

let audioCtx: AudioContext | null = null;

/** Handle a terminal bell according to the current setting. */
export function ringBell(el: HTMLElement): void {
  if (current.bell === "visual") {
    el.classList.remove("bell");
    void el.offsetWidth;
    el.classList.add("bell");
  } else if (current.bell === "sound") {
    try {
      audioCtx ??= new AudioContext();
      const osc = audioCtx.createOscillator();
      const gain = audioCtx.createGain();
      osc.frequency.value = 880;
      gain.gain.value = 0.05;
      osc.connect(gain).connect(audioCtx.destination);
      osc.start();
      osc.stop(audioCtx.currentTime + 0.08);
    } catch {
      /* ignore */
    }
  }
}
