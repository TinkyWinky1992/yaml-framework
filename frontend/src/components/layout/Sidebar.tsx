import { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, FileCode2, FileJson, FileText, Folder, FolderOpen, RefreshCw } from 'lucide-react';

interface Props {
  files: string[];
  selected: string | null;
  modifiedFiles: Set<string>;
  onSelect: (path: string) => void;
  onRefresh: () => void;
}

interface TreeNode {
  name: string;
  path: string;
  children: Map<string, TreeNode>;
  isFile: boolean;
}

function buildTree(files: string[]): TreeNode {
  const root: TreeNode = { name: '', path: '', children: new Map(), isFile: false };
  for (const file of [...files].sort()) {
    let node = root;
    const parts = file.split('/');
    parts.forEach((part, i) => {
      const path = parts.slice(0, i + 1).join('/');
      let child = node.children.get(part);
      if (!child) {
        child = { name: part, path, children: new Map(), isFile: i === parts.length - 1 };
        node.children.set(part, child);
      }
      node = child;
    });
  }
  return root;
}

function sortedChildren(node: TreeNode): TreeNode[] {
  return [...node.children.values()].sort((a, b) =>
    a.isFile === b.isFile ? a.name.localeCompare(b.name) : a.isFile ? 1 : -1,
  );
}

function FileIcon({ name }: { name: string }) {
  if (/\.ya?ml$/.test(name)) return <FileJson size={14} className="shrink-0 text-amber-400" />;
  if (/\.(ts|tsx|js|mjs|jsx)$/.test(name)) return <FileCode2 size={14} className="shrink-0 text-sky-400" />;
  return <FileText size={14} className="shrink-0 text-zinc-400" />;
}

export default function Sidebar({ files, selected, modifiedFiles, onSelect, onRefresh }: Props) {
  const tree = useMemo(() => buildTree(files), [files]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const toggle = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const renderNode = (node: TreeNode, depth: number) => {
    const pad = { paddingLeft: 8 + depth * 12 };
    if (node.isFile) {
      const active = node.path === selected;
      return (
        <button
          key={node.path}
          data-testid={`file-${node.path}`}
          onClick={() => onSelect(node.path)}
          style={pad}
          className={`flex w-full items-center gap-1.5 py-[3px] pr-2 text-left text-[13px] ${
            active ? 'bg-indigo-600/25 text-zinc-50' : 'text-zinc-300 hover:bg-zinc-800/70'
          }`}
        >
          <FileIcon name={node.name} />
          <span className="truncate">{node.name}</span>
          {modifiedFiles.has(node.path) && <span className="ml-auto text-[10px] font-bold text-amber-400">M</span>}
        </button>
      );
    }
    const open = !collapsed.has(node.path);
    return (
      <div key={node.path}>
        <button
          onClick={() => toggle(node.path)}
          style={pad}
          className="flex w-full items-center gap-1 py-[3px] pr-2 text-left text-[13px] text-zinc-300 hover:bg-zinc-800/70"
        >
          {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          {open ? (
            <FolderOpen size={14} className="text-indigo-400" />
          ) : (
            <Folder size={14} className="text-indigo-400" />
          )}
          <span className="truncate">{node.name}</span>
        </button>
        {open && sortedChildren(node).map((c) => renderNode(c, depth + 1))}
      </div>
    );
  };

  return (
    <aside className="flex h-full flex-col bg-ide-panel">
      <div className="panel-title justify-between">
        <span>Explorer</span>
        <button className="rounded p-0.5 hover:bg-zinc-800" onClick={onRefresh} title="Refresh workspace">
          <RefreshCw size={12} />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto pb-4">
        {files.length === 0 ? (
          <p className="px-3 text-xs text-zinc-500">No files found. Is the backend running?</p>
        ) : (
          sortedChildren(tree).map((n) => renderNode(n, 0))
        )}
      </div>
    </aside>
  );
}
