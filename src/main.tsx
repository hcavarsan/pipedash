import { MantineProvider, v8CssVariablesResolver } from '@mantine/core'
import { ModalsProvider } from '@mantine/modals'
import { Notifications } from '@mantine/notifications'
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import App from './App'
import { ErrorBoundary } from './components/ErrorBoundary'
import { PluginProvider } from './contexts/PluginContext'
import { BASE_PATH } from './lib/basePath'
import { queryClient } from './lib/queryClient'
import { theme } from './theme'

import '@mantine/core/styles.layer.css'
import '@mantine/notifications/styles.layer.css'
import 'mantine-datatable/styles.layer.css'
import './App.css'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <MantineProvider theme={theme} defaultColorScheme="dark" cssVariablesResolver={v8CssVariablesResolver}>
        <BrowserRouter basename={BASE_PATH.replace(/\/$/, '') || '/'}>
          <ErrorBoundary scope="app">
            <ModalsProvider>
              <PluginProvider>
                <Notifications position="bottom-right" pauseResetOnHover="notification" />
                <App />
              </PluginProvider>
            </ModalsProvider>
          </ErrorBoundary>
        </BrowserRouter>
      </MantineProvider>

      {import.meta.env.DEV && <ReactQueryDevtools initialIsOpen={false} position="bottom" />}
    </QueryClientProvider>
  </StrictMode>
)
