/**
 * Dev-only mock of the Tauri IPC bridge: routes `invoke` calls to the
 * mock HTTP backend (scripts/mock-backend, or any server implementing
 * the same routes on port 1430). Loaded by mock.html, never bundled into
 * the shipped app.
 */

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: { invoke: Invoke };
  }
}

const BASE = "http://127.0.0.1:1430";

window.__TAURI_INTERNALS__ = {
  invoke: async (cmd: string, args: Record<string, unknown> = {}): Promise<unknown> => {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(args)) {
      params.set(key, String(value));
    }
    const res = await fetch(`${BASE}/${cmd}?${params.toString()}`);
    const text = await res.text();
    if (!res.ok) {
      throw new Error(text);
    }
    return JSON.parse(text);
  },
};

export {};
