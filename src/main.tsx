import { StrictMode } from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'
import { AppRoot } from './AppRoot'

import '@mantine/core/styles.layer.css'
import '@mantine/notifications/styles.layer.css'
import './App.css'

const root = document.getElementById('root')

if (!root) {
  throw new Error('Missing #root element')
}

ReactDOM.createRoot(root).render(
  <StrictMode>
    <AppRoot>
      <App />
    </AppRoot>
  </StrictMode>
)
