// Desktop (Tauri) integration. In the browser these helpers do nothing and the UI uses the Vite proxy.

export const isDesktop = (): boolean => '__TAURI_INTERNALS__' in window;

/** Address of the backend. The desktop app picks a free port at startup, so it must ask for it. */
export async function resolveApiBase(): Promise<string> {
  if (isDesktop()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string>('get_api_base');
  }
  return import.meta.env.VITE_API_BASE ?? '';
}
