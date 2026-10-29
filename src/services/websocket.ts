import { withBasePath } from '../lib/basePath'
import { logger } from '../lib/logger'
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

  if (apiUrl?.startsWith('http')) {
    return `${apiUrl.replace(/^http/, 'ws').replace(/\/api\/v1$/, '')}/api/v1/ws`
  }

  if (apiUrl?.startsWith('/')) {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'

    return `${protocol}//${window.location.host}${apiUrl}/ws`
  }

  return `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}${withBasePath('/api/v1/ws')}`
}

const WS_URL = getWsUrl()

class WebSocketClient {
  private ws: WebSocket | null = null
  private events = new EventTarget()
  private listenerCounts = new Map<string, number>()
  private reconnectAttempts = 0
  private reconnectDelay = 1000
  private isConnecting = false
  private shouldReconnect = true
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
    useAuthStore.subscribe(
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
      logger.debug('WebSocket', 'Skipping connection - running in Tauri mode')

      return
    }

    if (this.isLockedOut) {
      logger.debug('WebSocket', 'Locked out due to auth failures - use manualReconnect after unlock')

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
        logger.debug('WebSocket', 'Connected')
        this.isConnecting = false
        // Don't reset reconnectAttempts here - only after stable connection
        this.emitConnectionStatus('connected')
      }

      this.ws.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data)
          const { type, payload } = message

          if (type === 'auth-error') {
            logger.warn('WebSocket', 'Authentication error', payload?.message || 'Unknown error')
            const wasCleared = useAuthStore.getState().incrementFailure()

            if (wasCleared) {
              // Token was cleared due to repeated failures - lock out until manual reconnect
              this.isLockedOut = true
              this.disconnect()
            }

            this.emitConnectionStatus('disconnected')

            return
          }

          if (type) {
            this.events.dispatchEvent(new CustomEvent(type, { detail: payload }))
          }
        } catch (error) {
          logger.error('WebSocket', 'Failed to parse message', error)
        }
      }

      this.ws.onclose = (event) => {
        logger.debug('WebSocket', 'Disconnected', { code: event.code, reason: event.reason })
        this.isConnecting = false
        this.ws = null

        // Check if this was a rapid disconnect (connection closed too quickly)
        const connectionDuration = this.connectionOpenedAt ? Date.now() - this.connectionOpenedAt : 0
        const wasRapidDisconnect = connectionDuration < WebSocketClient.STABLE_CONNECTION_MS

        if (wasRapidDisconnect) {
          this.rapidDisconnectCount++
          logger.warn(
            'WebSocket',
            `Rapid disconnect detected (${this.rapidDisconnectCount}/${WebSocketClient.RAPID_DISCONNECT_THRESHOLD})`
          )

          if (this.rapidDisconnectCount >= WebSocketClient.RAPID_DISCONNECT_THRESHOLD) {
            logger.error('WebSocket', 'Too many rapid disconnects - locking out, clearing token')
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
          this.beginReconnect()
        }
      }

      this.ws.onerror = (error) => {
        logger.error('WebSocket', 'Error', error)
        this.isConnecting = false
      }
    } catch (error) {
      logger.error('WebSocket', 'Failed to connect', error)
      this.isConnecting = false

      if (this.shouldReconnect) {
        this.beginReconnect()
      }
    }
  }

  private beginReconnect(): void {
    this.reconnectAttempts++
    this.emitConnectionStatus('reconnecting')
    this.scheduleReconnect()
  }

  private scheduleReconnect(): void {
    const delay = Math.min(this.reconnectDelay * 1.5 ** (this.reconnectAttempts - 1), 30000)

    logger.debug('WebSocket', `Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`)

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
  }

  dispose(): void {
    this.disconnect()
    this.events = new EventTarget()
    this.listenerCounts.clear()
  }

  listen<T = unknown>(eventType: string, callback: EventCallback<T>): UnlistenFn {
    if (isTauriMode()) {
      logger.debug('WebSocket', 'Skipping listener registration - running in Tauri mode')

      return () => undefined
    }

    const handler = (event: Event): void => {
      if (event instanceof CustomEvent) {
        try {
          callback(event.detail)
        } catch (error) {
          logger.error('WebSocket', `Error in listener for ${eventType}`, error)
        }
      }
    }

    this.events.addEventListener(eventType, handler)
    this.listenerCounts.set(eventType, (this.listenerCounts.get(eventType) ?? 0) + 1)

    if (!this.ws && !this.isConnecting) {
      this.connect()
    }

    let active = true

    return () => {
      if (!active) {
        return
      }

      active = false
      this.events.removeEventListener(eventType, handler)

      const remaining = (this.listenerCounts.get(eventType) ?? 1) - 1

      if (remaining > 0) {
        this.listenerCounts.set(eventType, remaining)
      } else {
        this.listenerCounts.delete(eventType)
      }
    }
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN
  }

  private emitConnectionStatus(status: 'connected' | 'disconnected' | 'reconnecting'): void {
    this.events.dispatchEvent(
      new CustomEvent('connection-status', { detail: { status, reconnectAttempts: this.reconnectAttempts } })
    )
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
  PROVIDER_STATUS_UPDATED: 'provider-status-updated',
  PROVIDER_HEALTH_ALERT: 'provider-health-alert',
  PIPELINE_TRIGGERED: 'pipeline-triggered',
  RUN_CANCELLED: 'run-cancelled',
  CONNECTION_STATUS: 'connection-status',
} as const
