import { lazy, type ReactNode } from 'react'
import { Route, Routes } from 'react-router-dom'

import { App } from './App'
import { LazyRoute } from './components/common/LazyRoute'
import { NativeIntentBridge } from './components/NativeIntentBridge'
import { ServerAccessGate } from './features/connection/components/ServerAccessGate'
import { WelcomeScreen } from './features/connection/components/WelcomeScreen'
import { getConnection } from './services/connection'

const ConnectServerScreen = lazy(() =>
  import('./features/connection/components/ConnectServerScreen').then((m) => ({
    default: m.ConnectServerScreen,
  }))
)

export function Root(): ReactNode {
  return (
    <>
      <NativeIntentBridge pairing />
      <RootScreens />
    </>
  )
}

function RootScreens(): ReactNode {
  const { mode } = getConnection()

  if (mode === 'unset') {
    return (
      <Routes>
        <Route
          path="/connect"
          element={
            <LazyRoute>
              <ConnectServerScreen />
            </LazyRoute>
          }
        />
        <Route path="*" element={<WelcomeScreen />} />
      </Routes>
    )
  }

  if (mode === 'client') {
    return (
      <ServerAccessGate>
        <App />
      </ServerAccessGate>
    )
  }

  return <App />
}
