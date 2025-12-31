import path from 'node:path'
import { visualizer } from 'rollup-plugin-visualizer'
import { defineConfig, type Plugin, type UserConfig } from 'vite'
import tsconfigPaths from 'vite-tsconfig-paths'

import { codecovVitePlugin } from '@codecov/vite-plugin'
import terser from '@rollup/plugin-terser'
import react from '@vitejs/plugin-react-swc'

const asPlugin = (p: any) => p as Plugin

const terserConfig = {
  mangle: true,
  output: { comments: false },
  compress: {
    drop_console: true,
    drop_debugger: true,
    pure_funcs: ['console.info', 'console.debug', 'console.warn'],
    passes: 2
  }
}


const createManualChunks = (id: string) => {
  if (!id.includes('node_modules')) {
    return
  }

  // Bundle Tauri separately (it doesn't depend on React)
  if (id.includes('@tauri-apps')) {
    return 'tauri'
  }

  // Bundle React, Mantine, and all UI libraries together
  // This ensures React is available when Mantine tries to use it
  if (
    id.includes('/react/') ||
    id.includes('/react-dom/') ||
    id.includes('/scheduler/') ||
    id.includes('@mantine/') ||
    id.includes('mantine-datatable') ||
    id.includes('@tabler/icons-react') ||
    id.includes('@emotion')
  ) {
    return 'vendor'
  }

  // Everything else
  return 'vendor'
}

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  resolve: {
    alias: { '@': path.resolve(__dirname, 'src') }
  },

  define: {
    global: 'window',
  },

  plugins: [
    asPlugin(react()),
    asPlugin(tsconfigPaths()),
    ...(!process.env.TAURI_DEBUG ? [asPlugin(terser(terserConfig))] : []),
    codecovVitePlugin({
      enableBundleAnalysis: process.env.CODECOV_TOKEN !== undefined,
      bundleName: 'pipedash',
      uploadToken: process.env.CODECOV_TOKEN,
      gitService: 'github',
    }),
    ...(process.env.ANALYZE ? [
      visualizer({
        open: true,
        gzipSize: true,
        brotliSize: true,
        filename: 'dist/stats.html'
      })
    ] : [])
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
    proxy: process.env.TAURI_ARCH !== undefined ? undefined : {
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
    chunkSizeWarningLimit: 500,
    // Tauri uses Chromium on Windows and WebKit on macOS and Linux
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'terser' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      output: {
        manualChunks: createManualChunks,
        chunkFileNames: 'assets/[name]-[hash].js',
        entryFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash].[ext]'
      },
      treeshake: {
        moduleSideEffects: 'no-external',
        propertyReadSideEffects: false,
        tryCatchDeoptimization: false
      }
    }
  }
} as UserConfig)
