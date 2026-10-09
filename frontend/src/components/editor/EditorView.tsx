import { useEffect, useRef } from 'react';
import Editor, { DiffEditor, type OnMount } from '@monaco-editor/react';
import { Code2, GitCompare, X } from 'lucide-react';
import type { FilePatch } from '../../types';
import { languageFor } from '../../monaco';

interface Props {
  path: string | null;
  content: string;
  dirty: boolean;
  reviewPatch: FilePatch | null;
  onChange: (value: string) => void;
  onSave: () => void;
  onCloseReview: () => void;
}

const editorOptions = {
  fontSize: 13,
  fontFamily: "'JetBrains Mono', 'Cascadia Code', Consolas, monospace",
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  automaticLayout: true,
  tabSize: 2,
  renderWhitespace: 'selection' as const,
};

export default function EditorView({ path, content, dirty, reviewPatch, onChange, onSave, onCloseReview }: Props) {
  const saveRef = useRef(onSave);
  useEffect(() => {
    saveRef.current = onSave;
  }, [onSave]);

  const handleMount: OnMount = (editor, monaco) => {
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveRef.current());
  };

  if (reviewPatch) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-zinc-800 bg-ide-panel px-3 text-xs">
          <GitCompare size={14} className="text-indigo-400" />
          <span className="text-zinc-300">Reviewing patch:</span>
          <span className="font-mono text-zinc-100" data-testid="review-path">
            {reviewPatch.path}
          </span>
          <span className="chip bg-indigo-500/20 text-indigo-300">{reviewPatch.action}</span>
          <span className="ml-3 text-zinc-500">original ← → proposed</span>
          <button className="btn-ghost ml-auto" onClick={onCloseReview}>
            <X size={13} /> Close review
          </button>
        </div>
        <div className="flex-1">
          <DiffEditor
            theme="vs-dark"
            language={languageFor(reviewPatch.path)}
            original={reviewPatch.original_content ?? ''}
            modified={reviewPatch.patched_content}
            options={{ ...editorOptions, readOnly: true, renderSideBySide: true }}
          />
        </div>
      </div>
    );
  }

  if (!path) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-zinc-500">
        <Code2 size={40} className="text-zinc-700" />
        <p className="text-sm">Select a spec or source file from the Explorer.</p>
        <p className="text-xs text-zinc-600">
          Edit a YAML spec to see semantic changes and impact analysis in the Assistant panel.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center border-b border-zinc-800 bg-ide-panel">
        <div className="flex h-full items-center gap-2 border-r border-zinc-800 border-t-2 border-t-indigo-500 bg-ide-bg px-3 text-xs text-zinc-200">
          <span className="font-mono" data-testid="active-path">
            {path}
          </span>
          {dirty && <span className="h-2 w-2 rounded-full bg-amber-400" title="Unsaved changes" />}
        </div>
      </div>
      <div className="flex-1">
        <Editor
          key={path}
          theme="vs-dark"
          path={path}
          language={languageFor(path)}
          value={content}
          onChange={(v) => onChange(v ?? '')}
          onMount={handleMount}
          options={editorOptions}
        />
      </div>
    </div>
  );
}
