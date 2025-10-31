import path from 'node:path'
import { visualizer } from 'rollup-plugin-visualizer'
import { defineConfig, type Plugin, type UserConfig } from 'vite'
import tsconfigPaths from 'vite-tsconfig-paths'

import { codecovVitePlugin } from '@codecov/vite-plugin'
import terser from '@rollup/plugin-terser'
import react from '@vitejs/plugin-react-swc'

const asPlugin = (p: any) => p as Plugin

const terserConfig = {
  compress: {
    drop_console: true,
    drop_debugger: true,
    pure_funcs: ['console.info', 'console.debug', 'console.warn'],
    passes: 2
  },
  mangle: {
    safari10: true
  },
  output: {
    comments: false
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

export default defineConfig({
  resolve: {
    alias: { '@': path.resolve(__dirname, 'src') },
    dedupe: ['react', 'react-dom']
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
    open: false,
    watch: {
      ignored: ['!**/node_modules/**']
    }
  },

  envPrefix: ['VITE_', 'TAURI_'],

  build: {
    outDir: 'dist',
    emptyOutDir: true,
    emptyOutDirExceptions: ['.gitkeep'],
    chunkSizeWarningLimit: 500,
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_DEBUG ? 'terser' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    commonjsOptions: {
      include: [/node_modules/],
      transformMixedEsModules: true
    },
    rollupOptions: {
      output: {
        chunkFileNames: 'assets/[name]-[hash].js',
        entryFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash].[ext]',
        manualChunks: createManualChunks
      },
      treeshake: {
        moduleSideEffects: true,
        propertyReadSideEffects: true,
        tryCatchDeoptimization: false
      }
    }
  }
} as UserConfig)
