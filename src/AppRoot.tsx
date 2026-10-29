import { ModalsProvider } from '@mantine/modals'
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import type { ReactNode } from 'react'
import { BrowserRouter } from 'react-router-dom'
import { ErrorBoundary } from './components/ErrorBoundary/ErrorBoundary'
import { NativeBridge } from './components/NativeBridge'
import { PrivacyCover } from './features/vault/components/PrivacyCover'
import { BASE_PATH } from './lib/basePath'
import { queryClient } from './lib/queryClient'
import { AppMantineProvider } from './theme/AppMantineProvider'
import { ToastHost } from './theme/ToastHost'

export function AppRoot({ children }: { children: ReactNode }): ReactNode {
  return (
    <QueryClientProvider client={queryClient}>
      <AppMantineProvider>
        <BrowserRouter basename={BASE_PATH.replace(/\/$/, '') || '/'}>
          <ErrorBoundary scope="app">
            <ModalsProvider>
              <ToastHost />
              {children}
              <NativeBridge />
            </ModalsProvider>
          </ErrorBoundary>
          <PrivacyCover />
        </BrowserRouter>
      </AppMantineProvider>

      {import.meta.env.DEV && <ReactQueryDevtools initialIsOpen={false} position="bottom" />}
    </QueryClientProvider>
  )
}
