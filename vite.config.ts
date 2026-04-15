import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { VitePWA } from 'vite-plugin-pwa'
import path from 'path'

const isWebBuild = process.env.VITE_BUILD_TARGET === 'web'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // The service worker + manifest only make sense for the web build. In
    // Tauri desktop the assets are already bundled, and a SW here would just
    // be dead code (Tauri's webview runs on custom schemes that don't play
    // well with SW registration anyway).
    ...(isWebBuild
      ? [
          VitePWA({
            registerType: 'autoUpdate',
            injectRegister: 'auto',
            // We author the manifest in `public/manifest.webmanifest` so it's
            // readable without a build step. Tell the plugin not to generate
            // its own copy.
            manifest: false,
            includeAssets: ['icons/icon-256.png', 'icons/icon-1024.png', 'vite.svg'],
            workbox: {
              // App assets (JS/CSS/fonts/images) — cache-first via precache.
              globPatterns: ['**/*.{js,css,html,svg,png,webmanifest,woff,woff2}'],
              // Never precache the SPA shell as precache — we want the SW to
              // fall back to index.html for every navigation so deep-links
              // like /atoms/:id work when the server is a static host.
              navigateFallback: '/index.html',
              // API and WS calls must NEVER be intercepted. Skip anything that
              // looks like a server endpoint the app talks to.
              navigateFallbackDenylist: [/^\/api\//, /^\/health/, /^\/ws/],
              runtimeCaching: [
                {
                  // Google Fonts stylesheets — cache for a day.
                  urlPattern: /^https:\/\/fonts\.googleapis\.com\//,
                  handler: 'StaleWhileRevalidate',
                  options: { cacheName: 'google-fonts-stylesheets' },
                },
                {
                  // Font files themselves — long-lived, cache for a year.
                  urlPattern: /^https:\/\/fonts\.gstatic\.com\//,
                  handler: 'CacheFirst',
                  options: {
                    cacheName: 'google-fonts-webfonts',
                    expiration: { maxAgeSeconds: 60 * 60 * 24 * 365, maxEntries: 30 },
                  },
                },
              ],
            },
          }),
        ]
      : []),
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    allowedHosts: true,
    watch: {
      // The Capacitor iOS scaffold copies dist-web into mobile/ios/App/App/public
      // during `cap sync`. Vite would otherwise crawl those files and get
      // confused by the stale absolute source paths in their sourcemaps.
      ignored: ['**/mobile/ios/**', '**/target/**', '**/dist-web/**', '**/dist/**'],
    },
    proxy: isWebBuild
      ? {
          '/api': {
            target: 'http://127.0.0.1:8080',
            changeOrigin: true,
          },
          '/health': {
            target: 'http://127.0.0.1:8080',
            changeOrigin: true,
          },
          '/ws': {
            target: 'ws://127.0.0.1:8080',
            ws: true,
            configure: (proxy) => {
              proxy.on('error', () => {});
            },
          },
        }
      : undefined,
  },
  resolve: isWebBuild
    ? {
        alias: {
          '@tauri-apps/api/core': path.resolve(__dirname, 'src/lib/stubs/tauri-core.ts'),
          '@tauri-apps/api/event': path.resolve(__dirname, 'src/lib/stubs/tauri-event.ts'),
          '@tauri-apps/plugin-dialog': path.resolve(__dirname, 'src/lib/stubs/tauri-dialog.ts'),
          '@tauri-apps/plugin-opener': path.resolve(__dirname, 'src/lib/stubs/tauri-opener.ts'),
          '@tauri-apps/plugin-fs': path.resolve(__dirname, 'src/lib/stubs/tauri-fs.ts'),
        },
      }
    : undefined,
})
