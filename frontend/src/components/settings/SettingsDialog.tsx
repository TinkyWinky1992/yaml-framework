import { Moon, RotateCcw, Sun, Wand2, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { DEFAULT_CUSTOM, type ThemeMode, type ThemeSettings } from '../../theme/theme';
import { useTheme } from '../../theme/ThemeContext';

interface Props {
  onClose: () => void;
}

const MODES: { id: ThemeMode; label: string; hint: string; icon: typeof Sun }[] = [
  { id: 'dark', label: 'Dark', hint: 'Default IDE look', icon: Moon },
  { id: 'light', label: 'Light', hint: 'Bright surfaces, dark text', icon: Sun },
  { id: 'custom', label: 'Custom', hint: 'Pick your own colors', icon: Wand2 },
];

export default function SettingsDialog({ onClose }: Props) {
  const { settings, setSettings, isDark } = useTheme();
  const dialogRef = useRef<HTMLDivElement>(null);

  // Escape closes the dialog, as in most desktop apps.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  const update = (patch: Partial<ThemeSettings>) => setSettings({ ...settings, ...patch });

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/70 p-4"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        className="w-full max-w-lg rounded-lg border border-zinc-700 bg-ide-panel p-5 text-zinc-200 shadow-2xl outline-none"
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 id="settings-title" className="text-sm font-semibold text-zinc-100">
            Settings
          </h2>
          <button className="btn-ghost" onClick={onClose} aria-label="Close settings">
            <X size={14} />
          </button>
        </div>

        <section>
          <h3 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Appearance</h3>
          <div role="radiogroup" aria-label="Theme" className="mt-2 grid grid-cols-3 gap-2">
            {MODES.map(({ id, label, hint, icon: Icon }) => {
              const active = settings.mode === id;
              return (
                <button
                  key={id}
                  role="radio"
                  aria-checked={active}
                  onClick={() => update({ mode: id })}
                  className={`flex flex-col items-start gap-1 rounded border p-3 text-left transition-colors ${
                    active
                      ? 'border-indigo-500 bg-indigo-500/10 text-zinc-100'
                      : 'border-zinc-700 hover:border-zinc-600 hover:bg-zinc-800/60'
                  }`}
                >
                  <span className="flex items-center gap-1.5 text-xs font-semibold">
                    <Icon size={13} /> {label}
                  </span>
                  <span className="text-[11px] text-zinc-500">{hint}</span>
                </button>
              );
            })}
          </div>
        </section>

        {settings.mode === 'custom' && (
          <section className="mt-5">
            <h3 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Custom colors</h3>
            <div className="mt-2 grid grid-cols-3 gap-3">
              <ColorField
                label="Background"
                value={settings.custom.background}
                onChange={(background) => update({ custom: { ...settings.custom, background } })}
              />
              <ColorField
                label="Text"
                value={settings.custom.foreground}
                onChange={(foreground) => update({ custom: { ...settings.custom, foreground } })}
              />
              <ColorField
                label="Accent"
                value={settings.custom.accent}
                onChange={(accent) => update({ custom: { ...settings.custom, accent } })}
              />
            </div>
            <div className="mt-3 rounded border border-zinc-700 bg-ide-bg p-3">
              <p className="text-xs text-zinc-200">Sample text in your theme</p>
              <p className="text-[11px] text-zinc-500">Secondary text and borders follow the same palette.</p>
              <div className="mt-2 flex items-center gap-2">
                <span className="btn-primary">Primary button</span>
                <span className="chip bg-indigo-500/20 text-indigo-300">Accent</span>
              </div>
            </div>
            <button
              className="btn-ghost mt-3"
              onClick={() => update({ custom: DEFAULT_CUSTOM })}
            >
              <RotateCcw size={13} /> Reset custom colors
            </button>
          </section>
        )}

        <p className="mt-5 text-[11px] text-zinc-500">
          Current look: {isDark ? 'dark' : 'light'}. Saved on this computer.
        </p>
      </div>
    </div>
  );
}

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="flex flex-col gap-1 text-[11px] text-zinc-400">
      {label}
      <div className="flex items-center gap-2 rounded border border-zinc-700 bg-zinc-900 px-2 py-1.5">
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="h-5 w-7 cursor-pointer border-0 bg-transparent p-0"
          aria-label={`${label} color`}
        />
        <span className="font-mono text-[11px] text-zinc-300">{value}</span>
      </div>
    </label>
  );
}
