import { type HotkeyItem, useHotkeys } from '@mantine/hooks'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useEffectEvent } from 'react'
import { logger } from '../lib/logger'
import { dialogOpen } from '../lib/overlays'
import { APP_COMMANDS } from '../lib/shortcuts'
import { isDesktopTauri } from '../utils/platform'

interface AppShortcuts {
  onSettings?: (() => void) | undefined
  onSidebar?: (() => void) | undefined
  onRefresh: () => void
  onLock?: (() => void) | undefined
}

function focusSearch() {
  if (dialogOpen()) {
    return
  }
  const input = document.querySelector<HTMLInputElement>('main [data-search-input]')
  input?.focus({ preventScroll: true })
  input?.select()
}

function whenNoDialog(action: (() => void) | undefined) {
  if (action && !dialogOpen()) {
    action()
  }
}

export function useAppShortcuts({ onSettings, onSidebar, onRefresh, onLock }: AppShortcuts): void {
  const desktop = isDesktopTauri()
  const optional: HotkeyItem[] = []
  if (onSettings) {
    optional.push(['mod+,', () => whenNoDialog(onSettings)])
  }
  if (onSidebar) {
    optional.push(['mod+\\', onSidebar])
  }
  if (desktop) {
    optional.push(['mod+R', onRefresh])
  }
  if (desktop && onLock) {
    optional.push(['mod+L', onLock])
  }
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
      ...optional,
    ],
    [],
    true
  )

  const onMenuCommand = useEffectEvent((payload: string) => {
    switch (payload) {
      case APP_COMMANDS.search:
        focusSearch()
        break
      case APP_COMMANDS.sidebar:
        onSidebar?.()
        break
    }
  })

  useEffect(() => {
    if (!desktop) {
      return
    }

    let active = true
    const subscription = listen<string>('menu-command', ({ payload }) => {
      if (active) {
        onMenuCommand(payload)
      }
    }).catch((error: unknown) => {
      logger.error('Shortcuts', 'Could not register native menu commands', error)

      return null
    })

    return () => {
      active = false
      void subscription.then((unlisten) => unlisten?.())
    }
  }, [desktop])
}
