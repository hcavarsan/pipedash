import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { logger } from './lib/logger'
import { getConnection, loadConnection } from './services/connection'
import { useNotificationStore } from './stores/notificationStore'

import '@mantine/core/styles.layer.css'
import '@mantine/notifications/styles.layer.css'
import './App.css'

const root = document.getElementById('root')

if (!root) {
  throw new Error('Missing #root element')
}

async function start(container: HTMLElement): Promise<void> {
  await loadConnection()

  const { mode, serverUrl } = getConnection()

  if (mode !== 'unset') {
    useNotificationStore.getState().selectSource(mode === 'client' ? `client:${serverUrl ?? ''}` : mode)
  }

  const [{ AppRoot }, { Root }] = await Promise.all([import('./AppRoot'), import('./Root')])

  ReactDOM.createRoot(container).render(
    <StrictMode>
      <AppRoot>
        <Root />
      </AppRoot>
    </StrictMode>
  )
}

start(root).catch((error: unknown) => {
  root.textContent = "Couldn't start Pipedash. Restart the app."
  logger.error('main', "Couldn't start Pipedash", error)
})
