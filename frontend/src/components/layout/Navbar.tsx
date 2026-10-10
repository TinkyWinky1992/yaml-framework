import { Cpu, FlaskConical, Loader2, Radar, RotateCcw, Save, Settings, Wifi, WifiOff } from 'lucide-react';
import type { ProvidersResponse } from '../../types';

interface Props {
  dirty: boolean;
  busy: string | null;
  connected: boolean;
  providers: ProvidersResponse | null;
  onSave: () => void;
  onRunTests: () => void;
  onScanDrift: () => void;
  onRollback: () => void;
  onProviderChange: (id: string) => void;
  onOpenSettings: () => void;
}

export default function Navbar(p: Props) {
  const isBusy = (key: string) => p.busy === key;
  return (
    <header className="flex h-11 shrink-0 items-center gap-3 border-b border-zinc-800 bg-ide-panel px-3">
      <div className="flex items-center gap-2 pr-2">
        <div className="flex h-6 w-6 items-center justify-center rounded bg-indigo-600 text-[11px] font-bold text-white">
          AI
        </div>
        <span className="text-sm font-semibold text-zinc-100">Spec IDE</span>
      </div>

      <div className="ml-2 flex items-center gap-1">
        <button className="btn-ghost" onClick={p.onSave} disabled={!p.dirty || p.busy !== null} title="Ctrl+S">
          {isBusy('save') ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
          Save
        </button>
        <button className="btn-ghost" onClick={p.onRunTests} disabled={p.busy !== null}>
          {isBusy('tests') ? <Loader2 size={14} className="animate-spin" /> : <FlaskConical size={14} />}
          Run Tests
        </button>
        <button className="btn-ghost" onClick={p.onScanDrift} disabled={p.busy !== null}>
          {isBusy('drift') ? <Loader2 size={14} className="animate-spin" /> : <Radar size={14} />}
          Scan Drift
        </button>
        <button className="btn-danger" onClick={p.onRollback} disabled={p.busy !== null}>
          {isBusy('rollback') ? <Loader2 size={14} className="animate-spin" /> : <RotateCcw size={14} />}
          Rollback
        </button>
      </div>

      <div className="ml-auto flex items-center gap-3">
        {p.providers && (
          <label className="flex items-center gap-1.5 text-xs text-zinc-400" title="AI provider used for Generate">
            <Cpu size={13} />
            <select
              data-testid="provider-select"
              className="rounded border border-zinc-700 bg-zinc-900 px-1.5 py-0.5 text-xs text-zinc-200 outline-none focus:border-indigo-500"
              value={p.providers.active}
              onChange={(e) => p.onProviderChange(e.target.value)}
            >
              {p.providers.providers.map((pr) => (
                <option key={pr.id} value={pr.id} disabled={!pr.available}>
                  {pr.label}
                  {pr.available ? '' : ' (not configured)'}
                </option>
              ))}
            </select>
          </label>
        )}
        <button className="btn-ghost" onClick={p.onOpenSettings} title="Settings (theme and more)" aria-label="Open settings">
          <Settings size={14} />
        </button>
        <span
          className={`flex items-center gap-1 text-xs ${p.connected ? 'text-emerald-400' : 'text-zinc-500'}`}
          title={p.connected ? 'Live file events connected' : 'Live file events disconnected'}
        >
          {p.connected ? <Wifi size={13} /> : <WifiOff size={13} />}
          {p.connected ? 'Live' : 'Offline'}
        </span>
      </div>
    </header>
  );
}
