// Main-thread client for the game worker. Exposes an async `call<T>(fn, ...args)` RPC
// with timeout, and auto-recreates the worker on fatal failures so the app stays usable.

type Pending = { resolve: (v: any) => void; reject: (err: Error) => void; timer: ReturnType<typeof setTimeout> };

const CALL_TIMEOUT_MS = 30_000;

let worker: Worker;
const pending = new Map<number, Pending>();
let nextId = 1;

function rejectAllPending(err: Error) {
  for (const p of pending.values()) {
    clearTimeout(p.timer);
    p.reject(err);
  }
  pending.clear();
}

function createWorker() {
  worker = new Worker(new URL('./game.worker.ts', import.meta.url), { type: 'module' });

  worker.onmessage = (ev: MessageEvent<{ id: number; ok: boolean; result?: any; error?: string }>) => {
    const { id, ok, result, error } = ev.data;
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    clearTimeout(p.timer);
    if (ok) p.resolve(result);
    else p.reject(new Error(error || 'worker error'));
  };

  worker.onerror = (ev) => {
    const err = new Error(ev.message || 'worker crashed');
    rejectAllPending(err);
    // Replace the faulted worker so subsequent calls have a fresh instance to message.
    try { worker.terminate(); } catch { /* already dead */ }
    createWorker();
  };
}

createWorker();

export function call<T = any>(fn: string, ...args: any[]): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      if (pending.delete(id)) {
        reject(new Error(`worker call timed out after ${CALL_TIMEOUT_MS}ms: ${fn}`));
      }
    }, CALL_TIMEOUT_MS);
    pending.set(id, { resolve, reject, timer });
    worker.postMessage({ id, fn, args });
  });
}
