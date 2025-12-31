import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'

import { MantineProvider } from '@mantine/core'
import { ModalsProvider } from '@mantine/modals'
import { Notifications } from '@mantine/notifications'
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'

import { ErrorBoundary } from './components/ErrorBoundary'
import { PluginProvider } from './contexts/PluginContext'
import { queryClient } from './lib/queryClient'
import App from './App'
import { theme } from './theme'

import '@mantine/core/styles.css'
import '@mantine/notifications/styles.css'
import 'mantine-datatable/styles.css'
import './styles/animations.css'
import './App.css'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <MantineProvider theme={theme} defaultColorScheme="dark">
        <BrowserRouter>
          <ErrorBoundary>
            <ModalsProvider>
              <PluginProvider>
                <Notifications position="top-right" />
                <App />
              </PluginProvider>
            </ModalsProvider>
          </ErrorBoundary>
        </BrowserRouter>
      </MantineProvider>

      {import.meta.env.DEV && (
        <ReactQueryDevtools initialIsOpen={false} position="bottom" />
      )}
    </QueryClientProvider>
  </StrictMode>
)
