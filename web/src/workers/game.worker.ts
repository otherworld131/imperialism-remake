// Dedicated worker that owns the WASM game engine instance.
// Receives { id, fn, args } messages and replies with { id, ok, result? , error? }.
import init, * as wasm from '../../../crates/wasm-bridge/pkg/wasm_bridge.js';

type Msg = { id: number; fn: string; args: any[] };
type Reply = { id: number; ok: true; result: any } | { id: number; ok: false; error: string };

let initPromise: Promise<void> | null = null;

async function ensureInit(): Promise<void> {
  if (!initPromise) {
    initPromise = init().then(() => undefined);
  }
  return initPromise;
}

self.onmessage = async (ev: MessageEvent<Msg>) => {
  const { id, fn, args } = ev.data;
  try {
    await ensureInit();
    const target = (wasm as any)[fn];
    if (typeof target !== 'function') {
      const reply: Reply = { id, ok: false, error: `Unknown WASM function: ${fn}` };
      (self as any).postMessage(reply);
      return;
    }
    const result = target(...args);
    const reply: Reply = { id, ok: true, result };
    (self as any).postMessage(reply);
  } catch (e) {
    const reply: Reply = { id, ok: false, error: String((e as Error)?.message ?? e) };
    (self as any).postMessage(reply);
  }
};
