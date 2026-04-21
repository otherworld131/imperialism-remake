/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    host: true,
    allowedHosts: true,
    fs: {
      // Allow serving WASM files from outside the web directory
      allow: ['..'],
    },
  },
  optimizeDeps: {
    exclude: ['wasm_bridge'],
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
    // The WASM bridge is not loadable from jsdom — exclude tests that touch it.
    setupFiles: ['./src/test-setup.ts'],
  },
})
