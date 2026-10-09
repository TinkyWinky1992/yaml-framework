import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api } from './api';
import { useFileEvents } from './hooks/useFileEvents';
import Navbar from './components/layout/Navbar';
import Sidebar from './components/layout/Sidebar';
import EditorView from './components/editor/EditorView';
import AssistantPanel from './components/assistant/AssistantPanel';
import BottomPanel from './components/bottom/BottomPanel';
import type {
  BottomTab,
  DriftDiagnostic,
  FileEvent,
  FilePatch,
  GenerationRecord,
  GitStatus,
  ImpactResult,
  ImplementationPlan,
  LogEntry,
  ProvidersResponse,
  SemanticChange,
  SpecNode,
  ValidationSummary,
} from './types';

const TEST_CHECKS: [string, string][] = [['node-tests', 'node --test "tests/**/*.test.mjs"']];

const errorMessage = (e: unknown) => (e instanceof Error ? e.message : String(e));
const isSpecFile = (path: string) => path.startsWith('specs/') && /\.ya?ml$/.test(path);

export default function App() {
  const [files, setFiles] = useState<string[]>([]);
  const [specs, setSpecs] = useState<SpecNode[]>([]);
  const [git, setGit] = useState<GitStatus | null>(null);
  const [providers, setProviders] = useState<ProvidersResponse | null>(null);

  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState('');
  const [saved, setSaved] = useState('');
  const dirty = selected !== null && content !== saved;

  const [changes, setChanges] = useState<SemanticChange[]>([]);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [impact, setImpact] = useState<ImpactResult | null>(null);
  const [plan, setPlan] = useState<ImplementationPlan | null>(null);
  const [reviewing, setReviewing] = useState<FilePatch | null>(null);

  const [validation, setValidation] = useState<ValidationSummary | null>(null);
  const [drift, setDrift] = useState<DriftDiagnostic[] | null>(null);
  const [history, setHistory] = useState<GenerationRecord[]>([]);
  const [bottomTab, setBottomTab] = useState<BottomTab>('log');
  const [log, setLog] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const logId = useRef(0);

  const spec = useMemo(() => specs.find((s) => s.file_path === selected) ?? null, [specs, selected]);

  const modifiedFiles = useMemo(() => {
    const set = new Set<string>();
    git?.status
      .split('\n')
      .filter(Boolean)
      .forEach((line) => set.add(line.slice(3).trim().replace(/^"|"$/g, '')));
    return set;
  }, [git]);

  const addLog = useCallback((level: LogEntry['level'], message: string) => {
    setLog((prev) => [
      ...prev.slice(-199),
      { id: ++logId.current, time: new Date().toLocaleTimeString(), level, message },
    ]);
  }, []);

  const run = useCallback(
    async (key: string, fn: () => Promise<void>) => {
      setBusy(key);
      try {
        await fn();
      } catch (e) {
        addLog('error', errorMessage(e));
        setBottomTab('log');
      } finally {
        setBusy(null);
      }
    },
    [addLog],
  );

  const refreshWorkspace = useCallback(async () => {
    const [ws, sp, gs, hist] = await Promise.all([api.workspace(), api.specs(), api.gitStatus(), api.history()]);
    setFiles(ws.files);
    setSpecs(sp);
    setGit(gs);
    setHistory(hist);
  }, []);

  useEffect(() => {
    refreshWorkspace()
      .then(() => addLog('info', 'Workspace loaded'))
      .catch((e) => addLog('error', `Backend unreachable: ${errorMessage(e)}`));
    api
      .providers()
      .then(setProviders)
      .catch(() => setProviders(null));
  }, [refreshWorkspace, addLog]);

  // Live semantic diff + impact while editing a spec.
  useEffect(() => {
    if (!spec || content === spec.raw_yaml) {
      setChanges([]);
      setImpact(null);
      setDiffError(null);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const res = await api.diff(spec.id, content);
        if (cancelled) return;
        setChanges(res.changes);
        setDiffError(null);
        const imp = res.changes.length > 0 ? await api.impact([spec.id]) : null;
        if (!cancelled) setImpact(imp);
      } catch (e) {
        if (!cancelled) setDiffError(errorMessage(e));
      }
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [content, spec]);

  const loadFile = useCallback(async (path: string) => {
    const res = await api.readFile(path);
    setSelected(path);
    setContent(res.content);
    setSaved(res.content);
  }, []);

  const selectFile = useCallback(
    (path: string) => {
      if (path === selected) return;
      if (dirty && !window.confirm(`Discard unsaved changes to ${selected}?`)) return;
      setReviewing(null);
      run('open', () => loadFile(path));
    },
    [selected, dirty, run, loadFile],
  );

  const persist = useCallback(
    async (path: string, text: string) => {
      if (isSpecFile(path)) {
        await api.saveSpec(path, text);
      } else {
        await api.saveFile(path, text);
      }
      setSaved(text);
      await refreshWorkspace();
    },
    [refreshWorkspace],
  );

  const save = useCallback(() => {
    if (!selected || !dirty) return;
    run('save', async () => {
      await persist(selected, content);
      addLog('success', `Saved ${selected}`);
    });
  }, [selected, dirty, content, run, persist, addLog]);

  const runTests = useCallback(
    () =>
      run('tests', async () => {
        addLog('info', `Running ${TEST_CHECKS.map(([, c]) => c).join(', ')}`);
        const summary = await api.runValidation(TEST_CHECKS);
        setValidation(summary);
        setBottomTab('tests');
        addLog(summary.overall_success ? 'success' : 'error', `Tests ${summary.overall_success ? 'passed' : 'failed'}`);
      }),
    [run, addLog],
  );

  const scanDrift = () =>
    run('drift', async () => {
      const result = await api.drift();
      setDrift(result);
      setBottomTab('drift');
      addLog(result.length ? 'error' : 'success', `Drift scan: ${result.length} issue(s)`);
    });

  const rollback = () => {
    const ok = window.confirm(
      'Rollback discards ALL uncommitted changes in the workspace (git checkout -- . && git clean -fd), ' +
        'including new untracked files. Continue?',
    );
    if (!ok) return;
    run('rollback', async () => {
      const res = await api.rollback();
      setPlan(null);
      setReviewing(null);
      await refreshWorkspace();
      if (selected) await loadFile(selected).catch(() => setSelected(null));
      addLog('success', res.message);
    });
  };

  const generate = (description: string) => {
    if (!spec) return;
    run('generate', async () => {
      addLog('info', `Generating plan for ${spec.id} via ${providers?.active ?? 'mock'}…`);
      const p = await api.generate(description, spec.id, content);
      setPlan(p);
      setReviewing(p.patches[0] ?? null);
      addLog('success', p.summary);
    });
  };

  const approve = () => {
    if (!plan) return;
    run('apply', async () => {
      await api.applyPatches(plan.patches);
      addLog('success', `Applied ${plan.patches.length} patch(es): ${plan.patches.map((p) => p.path).join(', ')}`);
      if (selected && isSpecFile(selected) && dirty) {
        await persist(selected, content);
        addLog('success', `Spec ${selected} saved alongside the implementation`);
      } else {
        await refreshWorkspace();
      }
      setPlan(null);
      setReviewing(null);
      const summary = await api.runValidation(TEST_CHECKS);
      setValidation(summary);
      setBottomTab('tests');
      addLog(summary.overall_success ? 'success' : 'error', `Post-apply tests ${summary.overall_success ? 'passed' : 'failed'}`);
    });
  };

  const changeProvider = (id: string) =>
    run('provider', async () => {
      setProviders(await api.setProvider(id));
      addLog('info', `AI provider set to ${id}`);
    });

  const eventTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const onFileEvent = useCallback(
    (event: FileEvent) => {
      addLog('info', `File ${event.kind}: ${event.paths.join(', ')}`);
      if (selected && !dirty && event.paths.includes(selected) && event.kind !== 'removed') {
        loadFile(selected).catch(() => undefined);
      }
      clearTimeout(eventTimer.current);
      eventTimer.current = setTimeout(() => refreshWorkspace().catch(() => undefined), 250);
    },
    [selected, dirty, loadFile, refreshWorkspace, addLog],
  );
  const connected = useFileEvents(onFileEvent);

  return (
    <div className="flex h-full flex-col">
      <Navbar
        branch={git?.branch ?? ''}
        changedFileCount={modifiedFiles.size}
        dirty={dirty}
        busy={busy}
        connected={connected}
        providers={providers}
        onSave={save}
        onRunTests={runTests}
        onScanDrift={scanDrift}
        onRollback={rollback}
        onProviderChange={changeProvider}
      />
      <div className="flex min-h-0 flex-1">
        <div className="w-60 shrink-0 border-r border-zinc-800">
          <Sidebar
            files={files}
            selected={selected}
            modifiedFiles={modifiedFiles}
            onSelect={selectFile}
            onRefresh={() => run('refresh', refreshWorkspace)}
          />
        </div>
        <main className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1">
            <EditorView
              path={selected}
              content={content}
              dirty={dirty}
              reviewPatch={reviewing}
              onChange={setContent}
              onSave={save}
              onCloseReview={() => setReviewing(null)}
            />
          </div>
          <div className="h-56 shrink-0 border-t border-zinc-800">
            <BottomPanel
              tab={bottomTab}
              onTab={setBottomTab}
              validation={validation}
              drift={drift}
              history={history}
              log={log}
              onOpenFile={selectFile}
            />
          </div>
        </main>
        <div className="w-80 shrink-0 border-l border-zinc-800">
          <AssistantPanel
            spec={spec}
            changes={changes}
            diffError={diffError}
            impact={impact}
            plan={plan}
            busy={busy}
            reviewing={reviewing}
            onGenerate={generate}
            onReview={setReviewing}
            onApprove={approve}
            onDiscardPlan={() => {
              setPlan(null);
              setReviewing(null);
            }}
          />
        </div>
      </div>
    </div>
  );
}
