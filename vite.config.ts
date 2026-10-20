import { codecovVitePlugin } from '@codecov/vite-plugin'
import react from '@vitejs/plugin-react-swc'
import { visualizer } from 'rollup-plugin-visualizer'
import { defineConfig } from 'vite'

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
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
            filename: 'dist/stats.html',
          }),
        ]
      : []),
  ],

  clearScreen: false,

  server: {
    port: 1420,
    strictPort: true,
    open: process.env.TAURI_ARCH === undefined,
    // if the host Tauri is expecting is set, use it
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
    // Only enable proxy in web mode (not in Tauri mode)
    // In Tauri mode, frontend uses IPC instead of HTTP API
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
    outDir: 'dist',
    emptyOutDir: false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rolldownOptions: {
      output: {
        minify: !process.env.TAURI_ENV_DEBUG && {
          mangle: true,
          compress: { dropConsole: true, dropDebugger: true },
        },
        comments: false,
        codeSplitting: {
          groups: [
            { name: 'tauri', test: /node_modules[\\/]@tauri-apps[\\/]/ },
            { name: 'charts', test: /node_modules[\\/](recharts|d3-[^\\/]+|victory-vendor)[\\/]/ },
            { name: 'mantine', test: /node_modules[\\/](@mantine|mantine-datatable)[\\/]/ },
            { name: 'icons', test: /node_modules[\\/]@tabler[\\/]/ },
            { name: 'vendor', test: /node_modules[\\/]/ },
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
