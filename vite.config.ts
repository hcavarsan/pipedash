import { codecovVitePlugin } from '@codecov/vite-plugin'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'
import { defineConfig } from 'vite'

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  base: './',

  resolve: {
    tsconfigPaths: true,
  },

  define: {
    global: 'window',
  },

  plugins: [
    react(),
    codecovVitePlugin({
      enableBundleAnalysis: process.env.CODECOV_TOKEN !== undefined,
      bundleName: 'pipedash',
      uploadToken: process.env.CODECOV_TOKEN,
      gitService: 'github',
    }),
    ...(process.env.ANALYZE
      ? [
          visualizer({
            open: true,
            gzipSize: true,
            brotliSize: true,
            filename: 'crates/pipedash-web/dist/stats.html',
          }),
        ]
      : []),
  ],

  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    open: process.env.TAURI_ARCH === undefined,
    host: host || '127.0.0.1',
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // tell vite to ignore watching Rust source and build directories
      ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'],
    },
    proxy:
      process.env.TAURI_ARCH !== undefined
        ? undefined
        : {
            '/api': {
              target: 'http://127.0.0.1:8080',
              changeOrigin: true,
              // WebSocket support for real-time updates
              ws: true,
            },
          },
  },

  envPrefix: ['VITE_', 'TAURI_ENV_*'],

  build: {
    outDir: 'crates/pipedash-web/dist',
    emptyOutDir: false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    chunkSizeWarningLimit: 4096,
    rolldownOptions: {
      output: {
        minify: !process.env.TAURI_ENV_DEBUG && {
          mangle: true,
          compress: { dropConsole: true, dropDebugger: true },
        },
        comments: false,
        codeSplitting: {
          groups: [
            { name: 'tauri', test: /node_modules[\\/]@tauri-apps[\\/]/, priority: 50 },
            { name: 'mantine', test: /node_modules[\\/]@mantine[\\/]/, priority: 40 },
            { name: 'icons', test: /node_modules[\\/]@tabler[\\/]/, priority: 40 },
            { name: 'vendor', test: /node_modules[\\/]/, tags: ['$initial'], priority: 30 },
            { name: 'charts', test: /node_modules[\\/](recharts|d3-[^\\/]+|victory-vendor)[\\/]/, priority: 20 },
            { name: 'monaco', test: /node_modules[\\/](monaco-editor|@monaco-editor)[\\/]/, priority: 20 },
            { name: 'dnd', test: /node_modules[\\/]@dnd-kit[\\/]/, priority: 20 },
            { name: 'html-to-image', test: /node_modules[\\/]html-to-image[\\/]/, priority: 20 },
            { name: 'datatable', test: /node_modules[\\/]mantine-datatable[\\/]/, priority: 20 },
            { name: 'lazy-vendor', test: /node_modules[\\/]/, priority: 10 },
          ],
        },
        chunkFileNames: 'assets/[name]-[hash].js',
        entryFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash].[ext]',
      },
      treeshake: {
        moduleSideEffects: 'no-external',
        propertyReadSideEffects: false,
      },
    },
  },
})
