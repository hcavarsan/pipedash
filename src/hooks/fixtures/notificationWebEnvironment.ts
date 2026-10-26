import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1200, height: 800 })
Object.defineProperty(window, 'isSecureContext', { configurable: true, value: true })

export const delivered: Array<{ title: string; body?: string }> = []

class BrowserNotification {
  static permission = 'granted'
  onclick: (() => void) | null = null

  constructor(title: string, options?: { body?: string }) {
    delivered.push({ title, body: options?.body })
  }
}

export class Socket {
  static OPEN = 1
  static instances: Socket[] = []
  readyState = 0
  onopen: (() => void) | null = null
  onclose: ((event: { code: number; reason: string }) => void) | null = null
  onmessage: ((event: { data: string }) => void) | null = null
  sent: string[] = []

  constructor() {
    Socket.instances.push(this)
  }

  open() {
    this.readyState = Socket.OPEN
    this.onopen?.()
  }

  send(data: string) {
    this.sent.push(data)
  }

  close() {
    this.readyState = 3
    this.onclose?.({ code: 1000, reason: 'Connection closed' })
  }

  emit(type: string, payload: unknown) {
    this.onmessage?.({ data: JSON.stringify({ type, payload }) })
  }
}

Object.assign(window, { Notification: BrowserNotification, WebSocket: Socket })
Object.assign(globalThis, { Notification: BrowserNotification, WebSocket: Socket })
