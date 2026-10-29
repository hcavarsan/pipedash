import { apiService } from './api'
import { isTauri } from './isTauri'
import { tauriService } from './tauri'
import { WS_EVENTS, wsClient } from './websocket'

export type PipedashService = Omit<typeof tauriService, 'setRefreshMode' | 'showSystemNotification'>

export const service: PipedashService = isTauri() ? tauriService : apiService

export const events = {
  onPipelinesUpdated: async <T = unknown>(callback: (payload: T) => void): Promise<() => void> => {
    if (isTauri()) {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<T>(WS_EVENTS.PIPELINES_UPDATED, (event) => {
        callback(event.payload)
      })

      return unlisten
    }

    return wsClient.listen<T>(WS_EVENTS.PIPELINES_UPDATED, callback)
  },

  onRefreshStatus: async <T = unknown>(callback: (payload: T) => void): Promise<() => void> => {
    if (isTauri()) {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<T>(WS_EVENTS.REFRESH_STATUS, (event) => {
        callback(event.payload)
      })

      return unlisten
    }

    return wsClient.listen<T>(WS_EVENTS.REFRESH_STATUS, callback)
  },

  init: (): void => {
    if (!isTauri()) {
      wsClient.connect()
    }
  },

  cleanup: (): void => {
    if (!isTauri()) {
      wsClient.dispose()
    }
  },

  listen: async <T = unknown>(eventName: string, callback: (payload: T) => void): Promise<() => void> => {
    if (isTauri()) {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<T>(eventName, (event) => {
        callback(event.payload)
      })

      return unlisten
    }

    return wsClient.listen<T>(eventName, callback)
  },
}
