
import { apiService } from './api'
import { tauriService } from './tauri'
import { WS_EVENTS, wsClient } from './websocket'

let _isTauriCache: boolean | null = null

export const isTauri = (): boolean => {
  if (_isTauriCache !== null) {
    return _isTauriCache
  }

  if (typeof window === 'undefined') {
    _isTauriCache = false
    
return _isTauriCache
  }

  const hasTauriInternals = '__TAURI_INTERNALS__' in window
  const hasTauri = '__TAURI__' in window


  _isTauriCache = hasTauriInternals || hasTauri

  if (import.meta.env.DEV) {
    console.debug('[isTauri] Detection (cached):', { hasTauriInternals, hasTauri, result: _isTauriCache })
  }

  return _isTauriCache
}

export type PipedashService = typeof tauriService

let _service: PipedashService | null = null

export const service: PipedashService = new Proxy({} as PipedashService, {
  get(_target, prop) {
    if (_service === null) {
      const inTauri = isTauri()


      _service = inTauri ? tauriService : apiService
      console.debug(`[Service] Initialized with ${inTauri ? 'Tauri IPC' : 'REST API'} backend`)
    }
    
return (_service as any)[prop]
  }
})

export const events = {
  onPipelinesUpdated: async <T = unknown>(
    callback: (payload: T) => void
  ): Promise<() => void> => {
    if (isTauri()) {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<T>(WS_EVENTS.PIPELINES_UPDATED, (event) => {
        callback(event.payload)
      })


      
return unlisten
    } 
      
return wsClient.listen<T>(WS_EVENTS.PIPELINES_UPDATED, callback)
    
  },

  onProvidersChanged: async <T = unknown>(
    callback: (payload: T) => void
  ): Promise<() => void> => {
    if (isTauri()) {
      const { listen } = await import('@tauri-apps/api/event')
      const unlisten = await listen<T>(WS_EVENTS.PROVIDERS_CHANGED, (event) => {
        callback(event.payload)
      })


      
return unlisten
    } 
      
return wsClient.listen<T>(WS_EVENTS.PROVIDERS_CHANGED, callback)
    
  },

  onRefreshStatus: async <T = unknown>(
    callback: (payload: T) => void
  ): Promise<() => void> => {
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
      wsClient.disconnect()
    }
  },

  listen: async <T = unknown>(
    eventName: string,
    callback: (payload: T) => void
  ): Promise<() => void> => {
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

export { apiService } from './api'
export { tauriService } from './tauri'
export { WS_EVENTS, wsClient } from './websocket'
