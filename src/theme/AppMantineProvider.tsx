import { MantineProvider, useMantineColorScheme, v8CssVariablesResolver } from '@mantine/core'
import { type ReactNode, useEffect, useLayoutEffect } from 'react'

import { ContextMenuHost } from '../components/common/ContextMenu'
import { useAccessibilityPrefs } from '../hooks/useAccessibilityPrefs'
import { useMobileShell } from '../hooks/useIsMobile'
import { useKeyboardInset } from '../hooks/useKeyboardInset'
import { useNativeChrome } from '../hooks/useNativeChrome'
import { useSheetDrag } from '../hooks/useSheetDrag'
import { mobileService } from '../services/mobile'
import { isMobilePlatform } from '../utils/platform'

import { theme } from './index'
import { iosCssVariablesResolver, iosTheme } from './ios'

function NativeAppearanceSync() {
  const { colorScheme } = useMantineColorScheme()

  useEffect(() => {
    if (!isMobilePlatform()) {
      return
    }

    mobileService.setAppearance(colorScheme === 'auto' ? 'system' : colorScheme).catch(() => undefined)
  }, [colorScheme])

  return null
}

export function AppMantineProvider({ children }: { children: ReactNode }) {
  const mobileShell = useMobileShell()

  useNativeChrome(mobileShell)
  useSheetDrag(mobileShell)
  useKeyboardInset(mobileShell)
  useAccessibilityPrefs()

  useLayoutEffect(() => {
    document.documentElement.toggleAttribute('data-ios', mobileShell)
    document.documentElement.style.removeProperty('background-color')
  }, [mobileShell])

  return (
    <MantineProvider
      theme={mobileShell ? iosTheme : theme}
      defaultColorScheme="dark"
      cssVariablesResolver={mobileShell ? iosCssVariablesResolver : v8CssVariablesResolver}
    >
      <NativeAppearanceSync />
      {children}
      {!mobileShell && <ContextMenuHost />}
    </MantineProvider>
  )
}
