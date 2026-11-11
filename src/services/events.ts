import { isTauri } from './isTauri'
import { wsClient } from './websocket'

export async function listenEvent<T = unknown>(eventName: string, callback: (payload: T) => void): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event')
    const unlisten = await listen<T>(eventName, (event) => {
      callback(event.payload)
    })

    return unlisten
  }

  return wsClient.listen<T>(eventName, callback)
}
