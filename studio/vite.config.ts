import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 27020, // Dedicated FaizDB Studio Port
    host: '0.0.0.0',
    proxy: {
      '/v1': {
        target: 'http://127.0.0.1:27018',
        changeOrigin: true,
      },
    },
  },
});
