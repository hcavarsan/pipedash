import React from 'react'
import ReactDOM from 'react-dom/client'

import { MantineProvider } from '@mantine/core'
import { ModalsProvider } from '@mantine/modals'
import { Notifications } from '@mantine/notifications'

import { ErrorBoundary } from './components/ErrorBoundary'
import { MediaQueryProvider } from './contexts/MediaQueryContext'
import { PluginProvider } from './contexts/PluginContext'
import { TableSchemaProvider } from './contexts/TableSchemaContext'
import App from './App'
import { theme } from './theme'

import '@mantine/core/styles.css'
import '@mantine/notifications/styles.css'
import 'mantine-datatable/styles.css'
import './styles/animations.css'
import './App.css'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <MantineProvider theme={theme} defaultColorScheme="dark">
      <ErrorBoundary>
        <ModalsProvider>
          <MediaQueryProvider>
            <PluginProvider>
              <TableSchemaProvider>
                <Notifications position="top-right" />
                <App />
              </TableSchemaProvider>
            </PluginProvider>
          </MediaQueryProvider>
        </ModalsProvider>
      </ErrorBoundary>
    </MantineProvider>
  </React.StrictMode>
)
