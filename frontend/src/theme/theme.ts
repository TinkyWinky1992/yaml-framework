// Theme engine. Every neutral and accent color in the UI is a CSS variable (see tailwind.config.js),
// so switching theme only means setting those variables on <html>.

export type ThemeMode = 'dark' | 'light' | 'custom';

export interface CustomColors {
  background: string;
  foreground: string;
  accent: string;
}

export interface ThemeSettings {
  mode: ThemeMode;
  custom: CustomColors;
}

export const DEFAULT_CUSTOM: CustomColors = {
  background: '#0f172a',
  foreground: '#e2e8f0',
  accent: '#6366f1',
};

export const DEFAULT_THEME: ThemeSettings = { mode: 'dark', custom: DEFAULT_CUSTOM };

const STORAGE_KEY = 'ai-ide.theme';
const SCALE_STEPS = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const;

type Rgb = [number, number, number];

// Hex tables for the built-in themes. Dark matches Tailwind's zinc and indigo defaults.
const DARK_ZINC: Record<number, string> = {
  50: '#fafafa', 100: '#f4f4f5', 200: '#e4e4e7', 300: '#d4d4d8', 400: '#a1a1aa',
  500: '#71717a', 600: '#52525b', 700: '#3f3f46', 800: '#27272a', 900: '#18181b', 950: '#09090b',
};
const LIGHT_ZINC: Record<number, string> = {
  50: '#09090b', 100: '#18181b', 200: '#27272a', 300: '#3f3f46', 400: '#52525b',
  500: '#71717a', 600: '#a1a1aa', 700: '#d4d4d8', 800: '#e4e4e7', 900: '#f4f4f5', 950: '#fafafa',
};
const DARK_INDIGO: Record<number, string> = {
  50: '#eef2ff', 100: '#e0e7ff', 200: '#c7d2fe', 300: '#a5b4fc', 400: '#818cf8',
  500: '#6366f1', 600: '#4f46e5', 700: '#4338ca', 800: '#3730a3', 900: '#312e81', 950: '#1e1b4b',
};

function hexToRgb(hex: string): Rgb {
  const clean = hex.replace('#', '');
  const full = clean.length === 3 ? clean.split('').map((c) => c + c).join('') : clean;
  const n = parseInt(full, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return [0, 1, 2].map((i) => Math.round(a[i] + (b[i] - a[i]) * t)) as Rgb;
}

/** "r g b" triplet, the form Tailwind's `rgb(var(--x) / <alpha-value>)` expects. */
const triplet = (c: Rgb) => c.join(' ');

const toRgbTriplet = (hex: string) => triplet(hexToRgb(hex));

function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex);
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/** Neutral scale running from `foreground` (50) to `background` (950). */
function customNeutrals(custom: CustomColors): Record<number, string> {
  const fg = hexToRgb(custom.foreground);
  const bg = hexToRgb(custom.background);
  const out: Record<number, string> = {};
  for (const step of SCALE_STEPS) {
    const [r, g, b] = mix(fg, bg, (step - 50) / 900);
    out[step] = `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  }
  return out;
}

/** Accent scale anchored at 600 (the button color), lighter above it and darker below. */
function accentScale(hex: string): Record<number, string> {
  const base = hexToRgb(hex);
  const out: Record<number, string> = {};
  for (const step of SCALE_STEPS) {
    const c =
      step <= 600
        ? mix(base, [255, 255, 255], ((600 - step) / 600) * 0.9)
        : mix(base, [0, 0, 0], ((step - 600) / 400) * 0.6);
    out[step] = `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
  }
  return out;
}

function scaleVars(prefix: string, scale: Record<number, string>): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const step of SCALE_STEPS) vars[`--${prefix}-${step}`] = toRgbTriplet(scale[step]);
  return vars;
}

/** Colors for the IDE's semantic tokens (ide-bg, ide-panel, ...), derived from the neutral scale. */
function ideVars(zinc: Record<number, string>, accent: string): Record<string, string> {
  return {
    '--ide-bg': toRgbTriplet(zinc[950]),
    '--ide-panel': toRgbTriplet(zinc[900]),
    '--ide-raised': toRgbTriplet(zinc[800]),
    '--ide-border': toRgbTriplet(zinc[700]),
    '--ide-accent': toRgbTriplet(accent),
  };
}


// Semantic hues (status colors). Dark uses Tailwind's defaults; light flips the scale so text stays readable on white.
const HUE_DEFAULTS: Record<'rose' | 'emerald' | 'amber' | 'sky', Record<number, string>> = {
  rose: { 50: '#fff1f2', 100: '#ffe4e6', 200: '#fecdd3', 300: '#fda4af', 400: '#fb7185', 500: '#f43f5e', 600: '#e11d48', 700: '#be123c', 800: '#9f1239', 900: '#881337', 950: '#4c0519' },
  emerald: { 50: '#ecfdf5', 100: '#d1fae5', 200: '#a7f3d0', 300: '#6ee7b7', 400: '#34d399', 500: '#10b981', 600: '#059669', 700: '#047857', 800: '#065f46', 900: '#064e3b', 950: '#022c22' },
  amber: { 50: '#fffbeb', 100: '#fef3c7', 200: '#fde68a', 300: '#fcd34d', 400: '#fbbf24', 500: '#f59e0b', 600: '#d97706', 700: '#b45309', 800: '#92400e', 900: '#78350f', 950: '#451a03' },
  sky: { 50: '#f0f9ff', 100: '#e0f2fe', 200: '#bae6fd', 300: '#7dd3fc', 400: '#38bdf8', 500: '#0ea5e9', 600: '#0284c7', 700: '#0369a1', 800: '#075985', 900: '#0c4a6e', 950: '#082f49' },
};
export const HUE_NAMES = Object.keys(HUE_DEFAULTS) as (keyof typeof HUE_DEFAULTS)[];

function hueVars(isDark: boolean): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const name of HUE_NAMES) {
    for (const step of SCALE_STEPS) {
      const source = isDark ? step : 1000 - step;
      vars[`--${name}-${step}`] = toRgbTriplet(HUE_DEFAULTS[name][source]);
    }
  }
  return vars;
}

export interface ResolvedTheme {
  /** Whether the UI reads as dark. Drives color-scheme and the Monaco theme. */
  isDark: boolean;
  monacoTheme: 'vs' | 'vs-dark';
  vars: Record<string, string>;
}

export function resolveTheme(settings: ThemeSettings): ResolvedTheme {
  if (settings.mode === 'light') {
    return {
      isDark: false,
      monacoTheme: 'vs',
      vars: {
        ...scaleVars('zinc', LIGHT_ZINC),
        ...scaleVars('indigo', DARK_INDIGO),
        ...ideVars(LIGHT_ZINC, DARK_INDIGO[500]),
        ...hueVars(false),
      },
    };
  }

  if (settings.mode === 'custom') {
    const zinc = customNeutrals(settings.custom);
    const accent = accentScale(settings.custom.accent);
    const isDark = luminance(settings.custom.background) < 0.5;
    return {
      isDark,
      monacoTheme: isDark ? 'vs-dark' : 'vs',
      vars: {
        ...scaleVars('zinc', zinc),
        ...scaleVars('indigo', accent),
        ...ideVars(zinc, accent[500]),
        ...hueVars(isDark),
      },
    };
  }

  return {
    isDark: true,
    monacoTheme: 'vs-dark',
    vars: {
      ...scaleVars('zinc', DARK_ZINC),
      ...scaleVars('indigo', DARK_INDIGO),
      ...ideVars(DARK_ZINC, DARK_INDIGO[500]),
      ...hueVars(true),
    },
  };
}

/** Applies the theme to the document. Safe to call repeatedly. */
export function applyTheme(settings: ThemeSettings): ResolvedTheme {
  const resolved = resolveTheme(settings);
  const root = document.documentElement;
  for (const [name, value] of Object.entries(resolved.vars)) {
    root.style.setProperty(name, value);
  }
  root.dataset.theme = settings.mode;
  root.style.colorScheme = resolved.isDark ? 'dark' : 'light';
  return resolved;
}

function isHexColor(value: unknown): value is string {
  return typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value);
}

/** Reads saved settings. Anything malformed falls back to the defaults. */
export function loadTheme(): ThemeSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_THEME;
    const parsed = JSON.parse(raw) as Partial<ThemeSettings>;
    const mode: ThemeMode = parsed.mode === 'light' || parsed.mode === 'custom' ? parsed.mode : 'dark';
    const custom = { ...DEFAULT_CUSTOM };
    for (const key of ['background', 'foreground', 'accent'] as const) {
      const value = parsed.custom?.[key];
      if (isHexColor(value)) custom[key] = value;
    }
    return { mode, custom };
  } catch {
    return DEFAULT_THEME;
  }
}

/** Saves settings. Storage can be unavailable (private windows), so failures are ignored. */
export function saveTheme(settings: ThemeSettings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // The theme still applies for this session.
  }
}
