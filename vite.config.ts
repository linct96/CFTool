import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  envPrefix: ['VITE_', 'TAURI_ENV_PLATFORM'],
  resolve: { tsconfigPaths: true },
  clearScreen: false,
  server: {
    host: 'localhost',
    port: 1420,
    strictPort: true,
    fs: {
      // Keep the legacy CLI's local files out of the development server.
      deny: [
        '.env',
        '.env.*',
        '*.{crt,pem,key,p12,pfx,cer,der}',
        '.npmrc',
        '.yarnrc.yml',
        '**/.git/**',
        '**/other/**',
      ],
    },
    watch: { ignored: ['**/src-tauri/**', '**/other/**'] },
  },
})
