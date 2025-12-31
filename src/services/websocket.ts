import { getToken, useAuthStore } from '../stores/authStore'

type EventCallback<T = unknown> = (payload: T) => void
type UnlistenFn = () => void

const isTauriMode = (): boolean => {
  if (typeof window === 'undefined') {
    return false
  }
  
return '__TAURI_INTERNALS__' in window || '__TAURI__' in window
}

const getWsUrl = (): string => {
  if (import.meta.env.VITE_WS_URL) {
    return import.meta.env.VITE_WS_URL
  }

  const apiUrl = import.meta.env.VITE_API_URL


  if (apiUrl && apiUrl.startsWith('http')) {
    return `${apiUrl.replace(/^http/, 'ws').replace(/\/api\/v1$/, '') }/api/v1/ws`
  }

  if (apiUrl && apiUrl.startsWith('/')) {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'


    
return `${protocol}//${window.location.host}${apiUrl}/ws`
  }

  return `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api/v1/ws`
}

const WS_URL = getWsUrl()

class WebSocketClient {
  private ws: WebSocket | null = null
  private listeners = new Map<string, Set<EventCallback>>()
  private reconnectAttempts = 0
  private reconnectDelay = 1000
  private isConnecting = false
  private shouldReconnect = true
  private tokenChangeUnsubscribe: (() => void) | null = null
  // Prevents reconnection after auth failures exceed threshold
  private isLockedOut = false
  // Track rapid disconnect cycles to detect server rejecting connections
  private connectionOpenedAt: number | null = null
  private rapidDisconnectCount = 0
  private static readonly STABLE_CONNECTION_MS = 5000
  private static readonly RAPID_DISCONNECT_THRESHOLD = 10

  constructor() {
    this.setupTokenChangeListener()
  }

  private setupTokenChangeListener(): void {
    if (isTauriMode()) {
      return
    }

    // Subscribe to auth state changes - only react to actual transitions
    this.tokenChangeUnsubscribe = useAuthStore.subscribe(
      (state) => state.isAuthenticated,
      (isAuthenticated, wasAuthenticated) => {
        // User just authenticated (unlocked vault)
        if (isAuthenticated && !wasAuthenticated) {
          if (this.ws?.readyState === WebSocket.OPEN) {
            this.reauthenticate()
          } else {
            this.manualReconnect()
          }
        } else if (!isAuthenticated && wasAuthenticated) {
          // User just logged out (was authenticated, now isn't)
          this.disconnect()
        }
        // Initial false state or same state = do nothing
      }
    )
  }

  connect(): void {
    if (isTauriMode()) {
      console.debug('[WebSocket] Skipping connection - running in Tauri mode')
      
return
    }

    if (this.isLockedOut) {
      console.debug('[WebSocket] Locked out due to auth failures - use manualReconnect after unlock')
      
return
    }

    if (this.ws?.readyState === WebSocket.OPEN || this.isConnecting) {
      return
    }

    this.isConnecting = true
    this.shouldReconnect = true

    try {
      this.ws = new WebSocket(WS_URL)

      this.ws.onopen = () => {
        this.connectionOpenedAt = Date.now()
        const token = getToken()

        if (token && this.ws) {
          this.ws.send(JSON.stringify({ type: 'auth', token }))
        }
        console.log('[WebSocket] Connected')
        this.isConnecting = false
        // Don't reset reconnectAttempts here - only after stable connection
        this.emitConnectionStatus('connected')
      }

      this.ws.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data)
          const { type, payload } = message

          if (type === 'auth-error') {
            console.warn('[WebSocket] Authentication error:', payload?.message || 'Unknown error')
            const wasCleared = useAuthStore.getState().incrementFailure()

            if (wasCleared) {
              // Token was cleared due to repeated failures - lock out until manual reconnect
              this.isLockedOut = true
              this.disconnect()
            }

            this.emitConnectionStatus('disconnected')
            
return
          }

          if (type && this.listeners.has(type)) {
            this.listeners.get(type)?.forEach((callback) => {
              try {
                return callback(payload)
              } catch (error) {
                return console.error(`[WebSocket] Error in listener for ${type}:`, error)
              }
            })
          }
        } catch (error) {
          console.error('[WebSocket] Failed to parse message:', error)
        }
      }

      this.ws.onclose = (event) => {
        console.log('[WebSocket] Disconnected:', event.code, event.reason)
        this.isConnecting = false
        this.ws = null

        // Check if this was a rapid disconnect (connection closed too quickly)
        const connectionDuration = this.connectionOpenedAt
          ? Date.now() - this.connectionOpenedAt
          : 0
        const wasRapidDisconnect = connectionDuration < WebSocketClient.STABLE_CONNECTION_MS

        if (wasRapidDisconnect) {
          this.rapidDisconnectCount++
          console.warn(
            `[WebSocket] Rapid disconnect detected (${this.rapidDisconnectCount}/${WebSocketClient.RAPID_DISCONNECT_THRESHOLD})`
          )

          if (this.rapidDisconnectCount >= WebSocketClient.RAPID_DISCONNECT_THRESHOLD) {
            console.error('[WebSocket] Too many rapid disconnects - locking out, clearing token')
            this.isLockedOut = true
            this.shouldReconnect = false
            useAuthStore.getState().clearToken()
            this.emitConnectionStatus('disconnected')
            
return
          }
        } else {
          // Connection was stable, reset counters
          this.rapidDisconnectCount = 0
          this.reconnectAttempts = 0
          this.reconnectDelay = 1000
        }

        this.connectionOpenedAt = null
        this.emitConnectionStatus('disconnected')

        if (this.shouldReconnect) {
          this.emitConnectionStatus('reconnecting')
          this.scheduleReconnect()
        }
      }

      this.ws.onerror = (error) => {
        console.error('[WebSocket] Error:', error)
        this.isConnecting = false
      }
    } catch (error) {
      console.error('[WebSocket] Failed to connect:', error)
      this.isConnecting = false

      if (this.shouldReconnect) {
        this.scheduleReconnect()
      }
    }
  }

  private scheduleReconnect(): void {
    this.reconnectAttempts++
    const delay = Math.min(this.reconnectDelay * 1.5 ** (this.reconnectAttempts - 1), 30000)

    console.log(
      `[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`
    )

    setTimeout(() => {
      if (this.shouldReconnect) {
        this.connect()
      }
    }, delay)
  }

  disconnect(): void {
    this.shouldReconnect = false
    if (this.ws) {
      this.ws.close()
      this.ws = null
    }
    this.listeners.clear()
  }

  listen<T = unknown>(eventType: string, callback: EventCallback<T>): UnlistenFn {
    if (isTauriMode()) {
      console.debug('[WebSocket] Skipping listener registration - running in Tauri mode')
      
return () => undefined
    }

    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set())
    }

    this.listeners.get(eventType)?.add(callback as EventCallback)

    if (!this.ws && !this.isConnecting) {
      this.connect()
    }

    return () => {
      this.listeners.get(eventType)?.delete(callback as EventCallback)

      if (this.listeners.get(eventType)?.size === 0) {
        this.listeners.delete(eventType)
      }
    }
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN
  }

  private emitConnectionStatus(status: 'connected' | 'disconnected' | 'reconnecting'): void {
    if (this.listeners.has('connection-status')) {
      this.listeners.get('connection-status')?.forEach((callback) => {
        try {
          return callback({ status, reconnectAttempts: this.reconnectAttempts })
        } catch (error) {
          console.error('[WebSocket] Error in connection-status listener:', error)
        }
      })
    }
  }

  manualReconnect(): void {
    this.isLockedOut = false
    this.rapidDisconnectCount = 0
    this.reconnectAttempts = 0
    this.reconnectDelay = 1000
    this.shouldReconnect = true
    this.connect()
  }

  reauthenticate(): void {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      return
    }

    const token = getToken()

    if (token) {
      this.ws.send(JSON.stringify({ type: 'auth', token }))
    }
  }
}

export const wsClient = new WebSocketClient()

export const WS_EVENTS = {
  PIPELINES_UPDATED: 'pipelines-updated',
  PROVIDERS_CHANGED: 'providers-changed',
  REFRESH_STATUS: 'refresh-status',
  PIPELINE_TRIGGERED: 'pipeline-triggered',
  RUN_CANCELLED: 'run-cancelled',
  CONNECTION_STATUS: 'connection-status',
} as const
