import type {
  DriftDiagnostic,
  FilePatch,
  GenerationRecord,
  ImpactResult,
  ImplementationPlan,
  ProvidersResponse,
  RepairReport,
  SemanticChange,
  SpecNode,
  ValidationSummary,
  WorkspaceResponse,
} from './types';

// Empty in dev (Vite proxy) and when served by the Rust server; absolute inside Tauri.
let API_BASE: string = import.meta.env.VITE_API_BASE ?? '';

/** Set once at startup (see main.tsx) before any request is made. */
export function setApiBase(base: string): void {
  API_BASE = base;
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...init?.headers },
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text || `${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

const post = <T>(path: string, body: unknown) =>
  request<T>(path, { method: 'POST', body: JSON.stringify(body) });

export const api = {
  workspace: () => request<WorkspaceResponse>('/api/workspace'),
  readFile: (path: string) =>
    request<{ path: string; content: string }>(`/api/file?path=${encodeURIComponent(path)}`),
  saveFile: (path: string, content: string) => post<{ success: boolean }>('/api/file', { path, content }),
  specs: () => request<SpecNode[]>('/api/specs'),
  saveSpec: (path: string, content: string) =>
    post<{ success: boolean; node: SpecNode }>('/api/specs', { path, content }),
  diff: (spec_id: string, new_yaml: string) =>
    post<{ changes: SemanticChange[] }>('/api/diff', { spec_id, new_yaml }),
  impact: (changed_spec_ids: string[]) => post<ImpactResult>('/api/impact', { changed_spec_ids }),
  generate: (description: string, spec_id: string, new_yaml: string) =>
    post<ImplementationPlan>('/api/generate', { description, spec_id, new_yaml }),
  repair: (body: {
    description: string;
    spec_id: string;
    new_yaml: string;
    checks: [string, string][];
    max_attempts: number;
  }) => post<RepairReport>('/api/repair', body),
  applyPatches: (patches: FilePatch[]) =>
    post<{ success: boolean; record_id: string }>('/api/patch/apply', { patches }),
  runValidation: (checks: [string, string][]) => post<ValidationSummary>('/api/validation/run', { checks }),
  rollback: () => post<{ success: boolean; message: string }>('/api/rollback', {}),
  drift: () => request<DriftDiagnostic[]>('/api/drift'),
  history: () => request<GenerationRecord[]>('/api/history'),
  providers: () => request<ProvidersResponse>('/api/providers'),
  setProvider: (id: string) => post<ProvidersResponse>('/api/providers', { id }),
};

export function wsUrl(path: string): string {
  const base = API_BASE || window.location.origin;
  return base.replace(/^http/, 'ws') + path;
}
