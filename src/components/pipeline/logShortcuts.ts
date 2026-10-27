import { getHotkeyHandler } from '@mantine/hooks'
import type { KeyboardEvent } from 'react'

import type { LogViewerHandle } from '../logs/LogViewer'

export function createLogShortcutHandler(viewer: () => LogViewerHandle | null) {
  return (event: KeyboardEvent<HTMLElement>) => {
    const handle = viewer()

    if (!handle || !event.currentTarget.contains(event.target as Node)) {
      return
    }

    getHotkeyHandler([
      ['mod+F', () => handle.focusSearch()],
      ['mod+G', () => handle.nextMatch()],
      ['shift+mod+G', () => handle.prevMatch()],
      ['F8', () => handle.nextError()],
      ['shift+F8', () => handle.prevError()],
      ['mod+Home', () => handle.scrollToStart()],
      ['mod+End', () => handle.scrollToEnd()],
    ])(event)
  }
}
