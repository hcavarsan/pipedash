import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { isRecord } from '../../utils/isRecord'

GlobalRegistrator.register({ width: 390, height: 844 })

interface Delivered {
  title: string
  body: string
  target: { key: string; pipelineId?: string }
}

interface NativeState {
  settings: { enabled: boolean; available: boolean; reason: string | null }
  calls: Array<{ command: string; args: Record<string, unknown> }>
  failure: Error | null
}

export const callbacks = new Map<number, (event: unknown) => void>()
export const listeners = new Map<string, number>()
export const delivered: Delivered[] = []
export const permission = { granted: true }
export const cachedPermission = { permission: 'default' }
export const native: NativeState = {
  settings: { enabled: false, available: true, reason: null },
  calls: [],
  failure: null,
}
let nextCallback = 0

Object.assign(window, {
  Notification: cachedPermission,
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: process.env.PIPEDASH_NOTIFICATION_PLATFORM ?? 'ios' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      const id = ++nextCallback
      callbacks.set(id, callback)
      return id
    },
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      if (command === 'plugin:event|listen') {
        listeners.set(String(args.event), Number(args.handler))
        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        listeners.delete(String(args.event))
        return
      }
      if (command === 'plugin:notification|is_permission_granted') {
        return permission.granted
      }
      if (command === 'show_system_notification') {
        const { title, body, target } = args
        if (
          typeof title === 'string' &&
          typeof body === 'string' &&
          isRecord(target) &&
          typeof target.key === 'string'
        ) {
          delivered.push({
            title,
            body,
            target:
              typeof target.pipelineId === 'string'
                ? { key: target.key, pipelineId: target.pipelineId }
                : { key: target.key },
          })
        }
        return
      }
      if (command === 'get_local_notification_settings') {
        native.calls.push({ command, args })
        return { ...native.settings }
      }
      if (command === 'set_local_notifications_enabled') {
        native.calls.push({ command, args })
        if (native.failure) {
          throw native.failure
        }
        native.settings = { ...native.settings, enabled: args.enabled === true }
        return { ...native.settings }
      }
      throw new Error(`Unexpected command: ${command}`)
    },
  },
})
