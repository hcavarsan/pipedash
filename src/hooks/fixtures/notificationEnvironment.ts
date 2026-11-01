import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { DesktopNotificationSnapshot } from '../../services/desktop'
import type { DesktopSettingsPatch, DesktopStatus } from '../../types/domain'
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
export const desktop: {
  status: DesktopStatus
  snapshot: DesktopNotificationSnapshot
  readGate: Promise<DesktopNotificationSnapshot> | null
  statusGate: Promise<DesktopStatus> | null
} = {
  status: {
    revision: 0,
    settings: { show_tray: false, keep_running_on_close: false, notifications_enabled: false },
    tray_state: 'off',
    ready: true,
    notification_preference_imported: false,
    error: null,
  },
  snapshot: { revision: 0, items: [] },
  readGate: null,
  statusGate: null,
}

interface Registration {
  mode: 'immediate' | 'reject' | 'deferred'
  gates: Array<PromiseWithResolvers<unknown>>
}

export const registration: Registration = { mode: 'immediate', gates: [] }
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
    invoke: async (command: string, args: Record<string, unknown> & { patch?: DesktopSettingsPatch } = {}) => {
      if (command === 'plugin:event|listen') {
        if (registration.mode === 'reject') {
          throw new Error('listen refused')
        }
        listeners.set(String(args.event), Number(args.handler))
        if (registration.mode === 'deferred') {
          const gate = Promise.withResolvers<unknown>()
          registration.gates.push(gate)

          return gate.promise.then(() => args.handler)
        }

        return args.handler
      }
      if (command === 'plugin:event|unlisten') {
        const event = String(args.event)
        if (listeners.get(event) === Number(args.eventId)) {
          listeners.delete(event)
        }
        callbacks.delete(Number(args.eventId))
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
      if (command === 'get_desktop_status') {
        native.calls.push({ command, args })
        if (!listeners.has('desktop-status-changed')) {
          throw new Error('Desktop settings fetched before subscription')
        }
        return desktop.statusGate ?? structuredClone(desktop.status)
      }
      if (command === 'import_desktop_notification_preference' || command === 'update_desktop_settings') {
        native.calls.push({ command, args })
        if (native.failure) {
          throw native.failure
        }
        if (command === 'import_desktop_notification_preference') {
          if (!desktop.status.notification_preference_imported) {
            desktop.status.settings.notifications_enabled = args.enabled === true
            desktop.status.notification_preference_imported = true
            desktop.status.revision += 1
          }
        } else {
          const patch = args.patch ?? {}
          desktop.status.settings = { ...desktop.status.settings, ...patch }
          desktop.status.notification_preference_imported ||= patch.notifications_enabled !== undefined
          desktop.status.revision += 1
        }
        return structuredClone(desktop.status)
      }
      if (command === 'get_desktop_notifications') {
        native.calls.push({ command, args })
        if (!listeners.has('desktop-notifications-changed')) {
          throw new Error('Desktop history fetched before subscription')
        }
        return desktop.readGate ?? structuredClone(desktop.snapshot)
      }
      if (command === 'mark_desktop_notifications_read' || command === 'clear_desktop_notifications') {
        native.calls.push({ command, args })
        desktop.snapshot = {
          revision: desktop.snapshot.revision + 1,
          items:
            command === 'clear_desktop_notifications'
              ? []
              : desktop.snapshot.items.map((item) =>
                  args.key === null || item.key === args.key ? { ...item, read: true } : item
                ),
        }
        return structuredClone(desktop.snapshot)
      }
      if (command === 'test_desktop_notification') {
        native.calls.push({ command, args })
        if (native.failure) {
          throw native.failure
        }
        return
      }
      throw new Error(`Unexpected command: ${command}`)
    },
  },
})
