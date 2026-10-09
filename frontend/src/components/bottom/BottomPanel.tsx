import { AlertCircle, AlertTriangle, CheckCircle2, History, Info, Radar, ScrollText, TestTube2, XCircle } from 'lucide-react';
import type { BottomTab, DriftDiagnostic, GenerationRecord, LogEntry, ValidationSummary } from '../../types';

interface Props {
  tab: BottomTab;
  onTab: (t: BottomTab) => void;
  validation: ValidationSummary | null;
  drift: DriftDiagnostic[] | null;
  history: GenerationRecord[];
  log: LogEntry[];
  onOpenFile: (path: string) => void;
}

const severityIcon = {
  error: <AlertCircle size={13} className="shrink-0 text-rose-400" />,
  warning: <AlertTriangle size={13} className="shrink-0 text-amber-400" />,
  info: <Info size={13} className="shrink-0 text-sky-400" />,
};

function TabButton({ active, onClick, icon, label, badge }: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  badge?: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-1.5 border-b-2 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider ${
        active ? 'border-indigo-500 text-zinc-100' : 'border-transparent text-zinc-500 hover:text-zinc-300'
      }`}
    >
      {icon}
      {label}
      {badge}
    </button>
  );
}

function Empty({ text }: { text: string }) {
  return <p className="p-3 text-xs text-zinc-500">{text}</p>;
}

export default function BottomPanel({ tab, onTab, validation, drift, history, log, onOpenFile }: Props) {
  const driftErrors = drift?.filter((d) => d.severity === 'error').length ?? 0;

  return (
    <section className="flex h-full flex-col bg-ide-panel">
      <nav className="flex shrink-0 border-b border-zinc-800">
        <TabButton
          active={tab === 'tests'}
          onClick={() => onTab('tests')}
          icon={<TestTube2 size={13} />}
          label="Tests"
          badge={
            validation &&
            (validation.overall_success ? (
              <CheckCircle2 size={12} className="text-emerald-400" />
            ) : (
              <XCircle size={12} className="text-rose-400" />
            ))
          }
        />
        <TabButton
          active={tab === 'drift'}
          onClick={() => onTab('drift')}
          icon={<Radar size={13} />}
          label="Drift"
          badge={drift && drift.length > 0 && <span className="chip bg-rose-500/20 text-rose-300">{drift.length}</span>}
        />
        <TabButton active={tab === 'history'} onClick={() => onTab('history')} icon={<History size={13} />} label="History" />
        <TabButton active={tab === 'log'} onClick={() => onTab('log')} icon={<ScrollText size={13} />} label="Output" />
      </nav>

      <div className="flex-1 overflow-y-auto font-mono text-[12px]" data-testid={`bottom-${tab}`}>
        {tab === 'tests' &&
          (!validation ? (
            <Empty text="No test run yet. Click “Run Tests” or approve a patch." />
          ) : (
            validation.checks.map((c) => (
              <div key={c.name} className="border-b border-zinc-800/60 p-2">
                <div className="mb-1 flex items-center gap-2">
                  {c.success ? (
                    <CheckCircle2 size={13} className="text-emerald-400" />
                  ) : (
                    <XCircle size={13} className="text-rose-400" />
                  )}
                  <span className="font-semibold text-zinc-200">{c.name}</span>
                  <span className="text-zinc-500">$ {c.command}</span>
                  <span className="ml-auto text-zinc-500">
                    exit {c.exit_code ?? '?'} · {c.duration_ms} ms
                  </span>
                </div>
                <pre className="whitespace-pre-wrap text-zinc-400">{(c.stdout + c.stderr).trim() || '(no output)'}</pre>
              </div>
            ))
          ))}

        {tab === 'drift' &&
          (drift === null ? (
            <Empty text="Drift not scanned yet. Click “Scan Drift”." />
          ) : drift.length === 0 ? (
            <div className="flex items-center gap-2 p-3 text-xs text-emerald-400">
              <CheckCircle2 size={14} /> Implementation matches all specification contracts.
            </div>
          ) : (
            <>
              <div className="px-2 pt-2 text-[11px] text-zinc-500">
                {driftErrors} error(s), {drift.length - driftErrors} warning(s)
              </div>
              {drift.map((d, i) => (
                <button
                  key={i}
                  onClick={() => onOpenFile(d.target_file)}
                  className="flex w-full items-start gap-2 border-b border-zinc-800/60 p-2 text-left hover:bg-zinc-800/40"
                >
                  {severityIcon[d.severity]}
                  <div className="min-w-0">
                    <div className="text-zinc-200">{d.message}</div>
                    <div className="text-zinc-500">
                      {d.spec_file} → {d.target_file} · expected: {d.expected}
                    </div>
                  </div>
                </button>
              ))}
            </>
          ))}

        {tab === 'history' &&
          (history.length === 0 ? (
            <Empty text="No generations applied yet." />
          ) : (
            history.map((h) => (
              <div key={h.id} className="flex items-center gap-3 border-b border-zinc-800/60 p-2">
                <span className="text-zinc-500">{new Date(h.timestamp).toLocaleString()}</span>
                <span className="chip bg-emerald-500/15 text-emerald-300">{h.status}</span>
                <span className="text-zinc-200">{h.plan_summary}</span>
                <span className="ml-auto truncate text-zinc-500">{h.patches.map((p) => p.path).join(', ')}</span>
              </div>
            ))
          ))}

        {tab === 'log' &&
          (log.length === 0 ? (
            <Empty text="No output yet." />
          ) : (
            [...log].reverse().map((l) => (
              <div key={l.id} className="flex gap-3 px-2 py-0.5">
                <span className="text-zinc-600">{l.time}</span>
                <span
                  className={
                    l.level === 'error' ? 'text-rose-400' : l.level === 'success' ? 'text-emerald-400' : 'text-zinc-300'
                  }
                >
                  {l.message}
                </span>
              </div>
            ))
          ))}
      </div>
    </section>
  );
}
