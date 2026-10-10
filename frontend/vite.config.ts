import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Must match the server's AI_IDE_PORT (defaults to 3001).
const backend = `http://127.0.0.1:${process.env.AI_IDE_PORT ?? 3001}`;

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': backend,
      '/ws': { target: backend.replace('http', 'ws'), ws: true },
    },
  },
  build: {
    chunkSizeWarningLimit: 4096,
  },
});
