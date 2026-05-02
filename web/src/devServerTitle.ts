const DEV_SERVER_STATUS_URL = '/dev-server-status.json';
const STATUS_POLL_INTERVAL_MS = 400;
const DEFAULT_TITLE = 'Imperialism Remake';

type DevServerPhase =
  | 'ready'
  | 'restarting'
  | 'compiling-web'
  | 'installing-deps'
  | 'building-wasm'
  | 'wasm-ready'
  | 'starting-dev-server'
  | 'failed';

type DevServerStatus = {
  phase?: DevServerPhase;
};

const STATUS_LABELS: Record<Exclude<DevServerPhase, 'ready' | 'wasm-ready'>, string> = {
  restarting: 'Restarting...',
  'compiling-web': 'Compiling...',
  'installing-deps': 'Installing deps...',
  'building-wasm': 'Compiling WASM...',
  'starting-dev-server': 'Starting dev server...',
  failed: 'Compilation failed',
};

export function formatDevServerTitle(status: DevServerStatus | null, baseTitle = DEFAULT_TITLE): string {
  const phase = status?.phase;
  if (!phase || phase === 'ready' || phase === 'wasm-ready') {
    return baseTitle;
  }

  return `${STATUS_LABELS[phase]} | ${baseTitle}`;
}

export function installDevServerTitleTracker(): void {
  if (!import.meta.env.DEV || typeof window === 'undefined') {
    return;
  }

  const baseTitle = document.title || DEFAULT_TITLE;
  let hasReachedStatusEndpoint = false;

  const applyTitle = (status: DevServerStatus | null) => {
    document.title = formatDevServerTitle(status, baseTitle);
  };

  const loadStatus = async () => {
    try {
      const response = await fetch(`${DEV_SERVER_STATUS_URL}?t=${Date.now()}`, {
        cache: 'no-store',
      });

      if (!response.ok) {
        if (!hasReachedStatusEndpoint) {
          applyTitle(null);
          return;
        }
        applyTitle({ phase: 'restarting' });
        return;
      }

      hasReachedStatusEndpoint = true;
      applyTitle(await response.json());
    } catch {
      if (hasReachedStatusEndpoint) {
        applyTitle({ phase: 'restarting' });
      }
    }
  };

  void loadStatus();
  const intervalId = window.setInterval(() => {
    void loadStatus();
  }, STATUS_POLL_INTERVAL_MS);

  const hot = import.meta.hot;
  hot?.on('vite:beforeUpdate', () => {
    applyTitle({ phase: 'compiling-web' });
  });
  hot?.on('vite:beforeFullReload', () => {
    applyTitle({ phase: 'starting-dev-server' });
  });
  hot?.on('vite:error', () => {
    applyTitle({ phase: 'failed' });
  });
  hot?.on('vite:afterUpdate', () => {
    void loadStatus();
  });

  window.addEventListener('beforeunload', () => {
    window.clearInterval(intervalId);
  }, { once: true });
}
