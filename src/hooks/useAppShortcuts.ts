import { useHotkeys } from '@mantine/hooks'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { logger } from '../lib/logger'
import { APP_COMMANDS } from '../lib/shortcuts'
import { isDesktopTauri } from '../utils/platform'

interface AppShortcuts {
  onSettings?: () => void
  onSidebar?: () => void
  onRefresh: () => void
  onLock?: () => void
}

function focusSearch() {
  const input = document.querySelector<HTMLInputElement>('main [data-search-input]')
  input?.focus({ preventScroll: true })
  input?.select()
}

export function useAppShortcuts({ onSettings, onSidebar, onRefresh, onLock }: AppShortcuts) {
  const desktop = isDesktopTauri()
  useHotkeys(
    [
      [
        '/',
        (event) => {
          const target = event.target
          if (
            target instanceof HTMLElement &&
            (target.closest('input, textarea, select') || target.isContentEditable)
          ) {
            return
          }
          event.preventDefault()
          focusSearch()
        },
        { preventDefault: false },
      ],
      ['mod+K', focusSearch],
      ...(onSettings ? ([['mod+,', onSettings]] as [string, () => void][]) : []),
      ...(onSidebar ? ([['mod+\\', onSidebar]] as [string, () => void][]) : []),
      ...(desktop ? ([['mod+R', onRefresh]] as [string, () => void][]) : []),
      ...(desktop && onLock ? ([['mod+L', onLock]] as [string, () => void][]) : []),
    ],
    [],
    true
  )

  useEffect(() => {
    if (!desktop) {
      return
    }
    const subscription = listen<string>('menu-command', ({ payload }) => {
      switch (payload) {
        case APP_COMMANDS.search:
          focusSearch()
          break
        case APP_COMMANDS.settings:
          onSettings?.()
          break
        case APP_COMMANDS.sidebar:
          onSidebar?.()
          break
        case APP_COMMANDS.refresh:
          onRefresh()
          break
        case APP_COMMANDS.lock:
          onLock?.()
          break
      }
    }).catch((error: unknown) => {
      logger.error('Shortcuts', 'Could not register native menu commands', error)
      return null
    })

    return () => {
      void subscription.then((unlisten) => unlisten?.())
    }
  }, [desktop, onSettings, onSidebar, onRefresh, onLock])
}
