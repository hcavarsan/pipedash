import { MantineProvider, v8CssVariablesResolver } from '@mantine/core'
import { Notifications } from '@mantine/notifications'
import { type ReactNode, useLayoutEffect } from 'react'

import { useMobileShell } from '../hooks/useIsMobile'
import { useKeyboardInset } from '../hooks/useKeyboardInset'
import { useNativeChrome } from '../hooks/useNativeChrome'
import { useSheetDrag } from '../hooks/useSheetDrag'

import { theme } from './index'
import { iosTheme, iosToastClassNames } from './ios'

export function AppMantineProvider({ children }: { children: ReactNode }) {
  const mobileShell = useMobileShell()

  useNativeChrome(mobileShell)
  useSheetDrag(mobileShell)
  useKeyboardInset(mobileShell)

  useLayoutEffect(() => {
    document.documentElement.toggleAttribute('data-ios', mobileShell)
  }, [mobileShell])

  return (
    <MantineProvider
      theme={mobileShell ? iosTheme : theme}
      defaultColorScheme="dark"
      cssVariablesResolver={v8CssVariablesResolver}
    >
      {children}
    </MantineProvider>
  )
}

export function ToastHost() {
  const mobileShell = useMobileShell()

  if (mobileShell) {
    return <Notifications position="top-center" limit={3} zIndex={600} classNames={iosToastClassNames} />
  }

  return <Notifications position="bottom-right" pauseResetOnHover="notification" />
}
