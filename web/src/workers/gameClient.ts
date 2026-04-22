// Main-thread client for the game worker. Exposes an async `call<T>(fn, ...args)` RPC.

type Pending = { resolve: (v: any) => void; reject: (err: Error) => void };

const worker = new Worker(new URL('./game.worker.ts', import.meta.url), { type: 'module' });
const pending = new Map<number, Pending>();
let nextId = 1;

worker.onmessage = (ev: MessageEvent<{ id: number; ok: boolean; result?: any; error?: string }>) => {
  const { id, ok, result, error } = ev.data;
  const p = pending.get(id);
  if (!p) return;
  pending.delete(id);
  if (ok) p.resolve(result);
  else p.reject(new Error(error || 'worker error'));
};

worker.onerror = (ev) => {
  const err = new Error(ev.message || 'worker crashed');
  for (const p of pending.values()) p.reject(err);
  pending.clear();
};

export function call<T = any>(fn: string, ...args: any[]): Promise<T> {
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, fn, args });
  });
}
