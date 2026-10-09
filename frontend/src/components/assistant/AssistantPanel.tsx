import { useState } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Eye,
  FileDiff,
  GitFork,
  Layers,
  Loader2,
  Minus,
  Pencil,
  Plus,
  Sparkles,
} from 'lucide-react';
import type { FilePatch, ImpactResult, ImplementationPlan, SemanticChange, SpecNode } from '../../types';

interface Props {
  spec: SpecNode | null;
  changes: SemanticChange[];
  diffError: string | null;
  impact: ImpactResult | null;
  plan: ImplementationPlan | null;
  busy: string | null;
  reviewing: FilePatch | null;
  onGenerate: (description: string) => void;
  onReview: (patch: FilePatch) => void;
  onApprove: () => void;
  onDiscardPlan: () => void;
}

const kindStyle = {
  added: { icon: Plus, cls: 'text-emerald-400 bg-emerald-500/10' },
  modified: { icon: Pencil, cls: 'text-amber-400 bg-amber-500/10' },
  removed: { icon: Minus, cls: 'text-rose-400 bg-rose-500/10' },
} as const;

function Section({ title, icon: Icon, children, count }: {
  title: string;
  icon: typeof Layers;
  count?: number;
  children: React.ReactNode;
}) {
  return (
    <section className="border-b border-zinc-800">
      <div className="panel-title">
        <Icon size={13} />
        {title}
        {count !== undefined && <span className="chip ml-auto bg-zinc-800 text-zinc-400">{count}</span>}
      </div>
      <div className="px-3 pb-3">{children}</div>
    </section>
  );
}

function FileList({ label, files, cls }: { label: string; files: string[]; cls: string }) {
  if (files.length === 0) return null;
  return (
    <div className="mb-2">
      <div className={`mb-1 text-[11px] font-semibold ${cls}`}>{label}</div>
      {files.map((f) => (
        <div key={f} className="truncate pl-2 font-mono text-[11px] text-zinc-300">
          {f}
        </div>
      ))}
    </div>
  );
}

export default function AssistantPanel(p: Props) {
  const [description, setDescription] = useState('');
  const generating = p.busy === 'generate';
  const applying = p.busy === 'apply';

  return (
    <aside className="flex h-full flex-col overflow-y-auto bg-ide-panel" data-testid="assistant-panel">
      <div className="panel-title border-b border-zinc-800">
        <Sparkles size={13} className="text-indigo-400" /> AI Assistant
      </div>

      {!p.spec ? (
        <p className="p-3 text-xs leading-relaxed text-zinc-500">
          Open a YAML file under <span className="font-mono text-zinc-400">specs/</span> to track semantic changes,
          analyze impact, and generate implementation patches.
        </p>
      ) : (
        <>
          <Section title="Semantic changes" icon={Layers} count={p.changes.length}>
            <div className="mb-2 font-mono text-[11px] text-zinc-500">{p.spec.id}</div>
            {p.diffError ? (
              <div className="rounded bg-rose-950/60 p-2 font-mono text-[11px] text-rose-300">{p.diffError}</div>
            ) : p.changes.length === 0 ? (
              <p className="text-xs text-zinc-500">No changes vs. registered spec. Start editing the YAML.</p>
            ) : (
              <ul className="space-y-1" data-testid="change-list">
                {p.changes.map((c, i) => {
                  const s = kindStyle[c.kind];
                  const Icon = s.icon;
                  return (
                    <li key={i} className="flex items-start gap-2 rounded bg-zinc-900 p-1.5">
                      <span className={`mt-0.5 rounded p-0.5 ${s.cls}`}>
                        <Icon size={11} />
                      </span>
                      <div className="min-w-0">
                        <div className="truncate font-mono text-[11px] text-zinc-200">{c.path.join('.')}</div>
                        <div className="text-[11px] text-zinc-500">{c.description}</div>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </Section>

          {p.impact && p.changes.length > 0 && (
            <Section title="Impact analysis" icon={GitFork}>
              <FileList label="Directly affected" files={p.impact.directly_affected} cls="text-rose-400" />
              <FileList label="Transitively affected" files={p.impact.transitively_affected} cls="text-amber-400" />
              <FileList label="Verification only (tests)" files={p.impact.verification_only} cls="text-sky-400" />
              <div className="text-[11px] text-zinc-500">{p.impact.unaffected_count} files unaffected</div>
            </Section>
          )}

          <Section title="Generate implementation" icon={Sparkles}>
            <textarea
              data-testid="task-description"
              className="mb-2 h-20 w-full resize-none rounded border border-zinc-700 bg-zinc-900 p-2 text-xs text-zinc-200 outline-none placeholder:text-zinc-600 focus:border-indigo-500"
              placeholder="Describe the task, e.g. 'Add phone field to getUser response'"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
            <button
              className="btn-primary w-full justify-center"
              data-testid="generate-btn"
              disabled={p.busy !== null || p.changes.length === 0}
              onClick={() => p.onGenerate(description.trim() || 'Implement specification changes')}
              title={p.changes.length === 0 ? 'Edit the spec first to produce semantic changes' : undefined}
            >
              {generating ? <Loader2 size={13} className="animate-spin" /> : <Sparkles size={13} />}
              Generate Patch
            </button>
          </Section>
        </>
      )}

      {p.plan && (
        <Section title="Implementation plan" icon={FileDiff} count={p.plan.patches.length}>
          <p className="mb-1 text-xs text-zinc-200" data-testid="plan-summary">
            {p.plan.summary}
          </p>
          <p className="mb-2 text-[11px] text-zinc-500">{p.plan.rationale}</p>

          <ul className="mb-2 space-y-1">
            {p.plan.patches.map((patch) => (
              <li
                key={patch.path}
                className={`flex items-center gap-2 rounded p-1.5 ${
                  p.reviewing?.path === patch.path ? 'bg-indigo-600/20' : 'bg-zinc-900'
                }`}
              >
                <span className="chip bg-zinc-800 text-zinc-400">{patch.action}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-zinc-200">{patch.path}</span>
                <button className="btn-ghost px-1.5 py-0.5" onClick={() => p.onReview(patch)} title="Review diff">
                  <Eye size={12} /> Review
                </button>
              </li>
            ))}
          </ul>

          {p.plan.potential_risks.length > 0 && (
            <div className="mb-2 rounded bg-amber-950/40 p-2">
              {p.plan.potential_risks.map((r) => (
                <div key={r} className="flex gap-1.5 text-[11px] text-amber-300">
                  <AlertTriangle size={12} className="mt-0.5 shrink-0" /> {r}
                </div>
              ))}
            </div>
          )}

          <div className="flex gap-2">
            <button
              className="btn-primary flex-1 justify-center bg-emerald-600 hover:bg-emerald-500"
              data-testid="approve-btn"
              onClick={p.onApprove}
              disabled={p.busy !== null || p.plan.patches.length === 0}
            >
              {applying ? <Loader2 size={13} className="animate-spin" /> : <CheckCircle2 size={13} />}
              Approve &amp; Apply
            </button>
            <button className="btn-ghost" onClick={p.onDiscardPlan} disabled={p.busy !== null}>
              Discard
            </button>
          </div>
        </Section>
      )}
    </aside>
  );
}
