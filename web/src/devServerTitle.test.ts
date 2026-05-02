import { describe, expect, it } from 'vitest';
import { formatDevServerTitle } from './devServerTitle';

describe('formatDevServerTitle', () => {
  it('keeps the base title when the dev server is ready', () => {
    expect(formatDevServerTitle({ phase: 'ready' })).toBe('Imperialism Remake');
    expect(formatDevServerTitle({ phase: 'wasm-ready' })).toBe('Imperialism Remake');
    expect(formatDevServerTitle(null)).toBe('Imperialism Remake');
  });

  it('renders a status prefix while compilation is in progress', () => {
    expect(formatDevServerTitle({ phase: 'restarting' })).toBe('Restarting... | Imperialism Remake');
    expect(formatDevServerTitle({ phase: 'compiling-web' })).toBe('Compiling... | Imperialism Remake');
    expect(formatDevServerTitle({ phase: 'installing-deps' })).toBe('Installing deps... | Imperialism Remake');
    expect(formatDevServerTitle({ phase: 'building-wasm' })).toBe('Compiling WASM... | Imperialism Remake');
    expect(formatDevServerTitle({ phase: 'starting-dev-server' })).toBe('Starting dev server... | Imperialism Remake');
  });

  it('renders a failure title when compilation fails', () => {
    expect(formatDevServerTitle({ phase: 'failed' })).toBe('Compilation failed | Imperialism Remake');
  });
});
