import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 390, height: 844 })

export const callbacks = new Map<number, (event: unknown) => void>()
export const listeners = new Map<string, number>()
export const delivered: Array<{ title: string; body: string; target: { key: string; pipelineId?: string } }> = []
export const permission = { granted: true }
export const cachedPermission = { permission: 'default' }
export const native = {
  settings: { enabled: false, available: true, reason: null as string | null },
  calls: [] as Array<{ command: string; args: Record<string, unknown> }>,
  failure: null as Error | null,
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
        listeners.set(args.event as string, args.handler as number)
        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        listeners.delete(args.event as string)
        return
      }
      if (command === 'plugin:notification|is_permission_granted') {
        return permission.granted
      }
      if (command === 'show_system_notification') {
        delivered.push(args as (typeof delivered)[number])
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
        native.settings = { ...native.settings, enabled: args.enabled as boolean }
        return { ...native.settings }
      }
      throw new Error(`Unexpected command: ${command}`)
    },
  },
})
