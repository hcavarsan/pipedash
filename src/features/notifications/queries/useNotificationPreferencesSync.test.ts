import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { NotificationPreferences, WatchOnce } from '../../../types/notifications'

GlobalRegistrator.register({ width: 1200, height: 800 })

const callbacks = new Map<number, (event: unknown) => void>()
const listeners = new Map<string, number>()
const calls: Array<{ command: string; preferences?: unknown }> = []
let nextCallback = 0
let serverCopy: NotificationPreferences | null = null
let fired: WatchOnce[] = []

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: (callback: (event: unknown) => void) => {
      nextCallback += 1
      callbacks.set(nextCallback, callback)

      return nextCallback
    },
    invoke: async (command: string, args: Record<string, unknown> = {}) => {
      switch (command) {
        case 'get_connection':
          return { mode: 'client', host: 'Studio', status: 'connected' }
        case 'plugin:event|listen':
          listeners.set(String(args.event), Number(args.handler))

          return args.handler
        case 'load_notification_preferences':
          calls.push({ command })

          return serverCopy
        case 'set_notification_preferences':
          calls.push({ command, preferences: args.preferences })

          return fired
        default:
          return undefined
      }
    },
  },
})

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { createElement } = await import('react')
const { loadConnection } = await import('../../../services/connection')
const { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } = await import('../../../stores/notificationStore')
const { useNotificationPreferencesSync } = await import('./useNotificationPreferencesSync')

await loadConnection()

const server: NotificationPreferences = { ...DEFAULT_NOTIFICATION_PREFERENCES, failed: false, muted_providers: [4] }
const sends = () => calls.filter(({ command }) => command === 'set_notification_preferences')
const loads = () => calls.filter(({ command }) => command === 'load_notification_preferences')
const unmount = () => act(async () => cleanup())

function Probe() {
  useNotificationPreferencesSync()

  return null
}

async function emitStatus(status: string) {
  const id = listeners.get('connection-status')
  assert.ok(id)
  await act(async () => {
    callbacks.get(id)?.({ event: 'connection-status', id: 1, payload: { status, reconnectAttempts: 0 } })
  })
}

const mount = () =>
  act(async () => {
    render(createElement(Probe))
  })

beforeEach(() => {
  calls.length = 0
  serverCopy = null
  fired = []
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES })
})

afterEach(async () => {
  await act(async () => cleanup())
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

test('client mode adopts the server copy on mount without sending it back', async () => {
  serverCopy = server
  await mount()

  await waitFor(() => assert.deepEqual(useNotificationStore.getState().preferences, server))
  await unmount()
  assert.equal(loads().length, 1)
  assert.deepEqual(sends(), [])
})

test('client mode sends the local copy when the server has none', async () => {
  const local = { ...DEFAULT_NOTIFICATION_PREFERENCES, long_run: false }
  useNotificationStore.setState({ preferences: local })
  await mount()

  await waitFor(() => assert.equal(sends().length, 1))
  assert.deepEqual(sends()[0]?.preferences, local)
  assert.deepEqual(useNotificationStore.getState().preferences, local)
  assert.equal(calls[0]?.command, 'load_notification_preferences')
})

test('every connected status loads first and adopts again, and other statuses do not', async () => {
  await mount()
  await waitFor(() => assert.equal(sends().length, 1))

  await emitStatus('reconnecting')
  assert.equal(loads().length, 1)

  serverCopy = server
  await emitStatus('connected')
  await waitFor(() => assert.deepEqual(useNotificationStore.getState().preferences, server))
  await unmount()
  assert.equal(loads().length, 2)
  assert.equal(sends().length, 1)
})

test('used one-time requests the app reports are marked fired', async () => {
  const watch = { pipeline_id: 'github__1__acme/api__ci.yml', since: '2026-11-04T10:00:00.000Z' }
  const used = { ...watch, fired_at: '2026-11-04T10:30:00.000Z' }
  fired = [used]
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [watch] } })
  await mount()

  await waitFor(() => assert.deepEqual(useNotificationStore.getState().preferences.watch_once, [used]))
})
