import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    fs: {
      // Allow serving WASM files from outside the web directory
      allow: ['..'],
    },
  },
  optimizeDeps: {
    exclude: ['wasm_bridge'],
  },
})
