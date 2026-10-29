import { ModalsProvider } from '@mantine/modals'
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { App } from './App'
import { ErrorBoundary } from './components/ErrorBoundary/ErrorBoundary'
import { NativeBridge } from './components/NativeBridge'
import { PrivacyCover } from './features/vault/components/PrivacyCover'
import { BASE_PATH } from './lib/basePath'
import { queryClient } from './lib/queryClient'
import { AppMantineProvider } from './theme/AppMantineProvider'
import { ToastHost } from './theme/ToastHost'

import '@mantine/core/styles.layer.css'
import '@mantine/notifications/styles.layer.css'
import './App.css'

const root = document.getElementById('root')

if (!root) {
  throw new Error('Missing #root element')
}

ReactDOM.createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AppMantineProvider>
        <BrowserRouter basename={BASE_PATH.replace(/\/$/, '') || '/'}>
          <ErrorBoundary scope="app">
            <ModalsProvider>
              <ToastHost />
              <App />
              <PrivacyCover />
              <NativeBridge />
            </ModalsProvider>
          </ErrorBoundary>
        </BrowserRouter>
      </AppMantineProvider>

      {import.meta.env.DEV && <ReactQueryDevtools initialIsOpen={false} position="bottom" />}
    </QueryClientProvider>
  </StrictMode>
)
