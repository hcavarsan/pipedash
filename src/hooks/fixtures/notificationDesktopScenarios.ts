import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'

import { useDesktopNotifications } from '../../features/notifications/queries/useDesktopNotifications'
import { useLocalNotificationsLifecycle } from '../../features/notifications/queries/useLocalNotificationQueries'
import { useDesktopLifecycle, useDesktopStatus } from '../../features/settings/queries/useDesktopQueries'
import { type CenterNotification, useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { useNotificationStore } from '../../stores/notificationStore'
import type { DesktopStatus } from '../../types/domain'
import { useFailureNotifications } from '../useFailureNotifications'
import { useProviderHealthAlerts } from '../useProviderHealthAlerts'

import { callbacks, delivered, desktop, listeners, native, permission } from './notificationEnvironment'
import { defineRegistrationScenarios } from './notificationRegistrationScenarios'

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const initialStatus = structuredClone(desktop.status)
const notice: CenterNotification = {
  key: 'watch:session-one',
  at: Date.now(),
  read: false,
  tone: 'danger',
  level: null,
  title: 'Watched run #42 failed',
  body: 'menu-01',
  providerIds: [1, 2],
  pipelineId: 'jenkins__1__menu-01',
  link: null,
  runNumber: 42,
}

function Screen() {
  useFailureNotifications()
  useProviderHealthAlerts()
  useLocalNotificationsLifecycle()
  useDesktopLifecycle()
  useDesktopNotifications()
  const status = useDesktopStatus()
  const items = useNotificationCenterStore((state) => state.items)
  return createElement(
    'output',
    null,
    `${status.data?.settings.notifications_enabled}:${items.map((item) => item.title).join(',')}`
  )
}

async function mount(notificationsEnabled = false) {
  useNotificationStore.setState({ notificationsEnabled })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Screen)))
  })
  await waitFor(() =>
    assert.ok(listeners.has('desktop-status-changed') && listeners.has('desktop-notifications-changed'))
  )
}

async function publish(name: string, payload: unknown) {
  await act(async () => {
    const id = listeners.get(name)
    if (id !== undefined) {
      callbacks.get(id)?.({ event: name, id, payload })
    }
  })
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  useNotificationCenterStore.setState({ items: [], selection: null, desktopRevision: -1 })
  useNotificationStore.setState({ notificationsEnabled: false })
  desktop.status = structuredClone(initialStatus)
  desktop.snapshot = { revision: 0, items: [] }
  desktop.readGate = null
  desktop.statusGate = null
  delivered.length = 0
  native.calls.length = 0
  native.failure = null
  permission.granted = true
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

defineRegistrationScenarios(client, Screen)

test('consent import waits for legacy store hydration', async (context) => {
  let hydrated: (() => void) | undefined
  context.mock.method(useNotificationStore.persist, 'hasHydrated', () => false)
  context.mock.method(
    useNotificationStore.persist,
    'onFinishHydration',
    (callback: Parameters<typeof useNotificationStore.persist.onFinishHydration>[0]) => {
      hydrated = () => callback(useNotificationStore.getState())
      return () => undefined
    }
  )
  await mount(false)
  await waitFor(() => assert.equal(typeof hydrated, 'function'))
  assert.equal(
    native.calls.some((call) => call.command === 'import_desktop_notification_preference'),
    false
  )
  useNotificationStore.setState({ notificationsEnabled: true })
  await act(async () => hydrated?.())
  await waitFor(() => assert.equal(desktop.status.settings.notifications_enabled, true))
})

for (const enabled of [true, false]) {
  test(`hydrated legacy ${enabled} consent imports once and native edits win on remount`, async () => {
    await mount(enabled)
    await waitFor(() => assert.equal(desktop.status.notification_preference_imported, true))
    assert.equal(desktop.status.settings.notifications_enabled, enabled)
    desktop.status.settings.notifications_enabled = !enabled
    desktop.status.revision += 1
    await act(async () => cleanup())
    await mount(enabled)
    await waitFor(() => assert.ok(document.body.textContent?.startsWith(String(!enabled))))
    assert.equal(native.calls.filter((call) => call.command === 'import_desktop_notification_preference').length, 1)
  })
}

test('failed consent persistence remains unacknowledged and retries on the next mount', async (context) => {
  context.mock.method(console, 'warn', () => undefined)
  native.failure = new Error('Settings are read-only')
  await mount(true)
  await waitFor(() => assert.ok(native.calls.some((call) => call.command === 'import_desktop_notification_preference')))
  assert.equal(desktop.status.notification_preference_imported, false)
  await waitFor(() => assert.ok(document.body.textContent?.startsWith('false')))
  await act(async () => cleanup())
  native.failure = null
  await mount(true)
  await waitFor(() => assert.equal(desktop.status.notification_preference_imported, true))
})

test('an existing settings error prevents consent import without discarding the fetched status', async () => {
  desktop.status.error = 'Desktop settings could not be read.'
  await mount(true)
  await waitFor(() => assert.ok(document.body.textContent?.startsWith('false')))
  assert.equal(desktop.status.notification_preference_imported, false)
  assert.equal(
    native.calls.some((call) => call.command === 'import_desktop_notification_preference'),
    false
  )
})

test('a newer settings event wins over an older initial fetch', async () => {
  const read = Promise.withResolvers<DesktopStatus>()
  desktop.statusGate = read.promise
  await mount()
  await waitFor(() => assert.ok(native.calls.some((call) => call.command === 'get_desktop_status')))
  const newer = {
    ...initialStatus,
    revision: 10,
    notification_preference_imported: true,
    settings: { ...initialStatus.settings, notifications_enabled: true },
  }
  await publish('desktop-status-changed', newer)
  await act(async () => read.resolve({ ...initialStatus, notification_preference_imported: true }))
  await waitFor(() => assert.ok(document.body.textContent?.startsWith('true')))
})

test('a newer history event survives an older fetch and preserves exact run metadata', async () => {
  const read = Promise.withResolvers<typeof desktop.snapshot>()
  desktop.readGate = read.promise
  await mount()
  await publish('desktop-notifications-changed', { revision: 8, items: [notice] })
  await act(async () => read.resolve({ revision: 1, items: [] }))
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
  assert.deepEqual(useNotificationCenterStore.getState().items, [notice])
  await publish('desktop-notifications-changed', { revision: 7, items: [] })
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
})

for (const enabled of [true, false]) {
  test(`native history remains visible with notifications ${enabled ? 'denied' : 'off'} without frontend delivery`, async () => {
    permission.granted = false
    desktop.snapshot = { revision: 1, items: [notice] }
    await mount(enabled)
    await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
    assert.deepEqual(delivered, [])
    assert.equal(listeners.has('pipelines-failed'), false)
    assert.equal(listeners.has('provider-health-alert'), false)
    assert.equal(
      native.calls.some((call) => call.command.includes('local_notification')),
      false
    )
  })
}

test('desktop read, open and clear actions use native snapshots rather than optimistic edits', async () => {
  desktop.snapshot = { revision: 1, items: [notice, { ...notice, key: 'second', runNumber: null }] }
  await mount()
  await waitFor(() => assert.equal(useNotificationCenterStore.getState().items.length, 2))
  act(() => useNotificationCenterStore.getState().open({ type: 'notification', key: notice.key }))
  await waitFor(() => assert.equal(useNotificationCenterStore.getState().items[0]?.read, true))
  assert.equal(useNotificationCenterStore.getState().selection?.type, 'notification')
  act(() => useNotificationCenterStore.getState().markAllRead())
  await waitFor(() => assert.ok(useNotificationCenterStore.getState().items.every((item) => item.read)))
  act(() => useNotificationCenterStore.getState().clear())
  await waitFor(() => assert.deepEqual(useNotificationCenterStore.getState().items, []))
  assert.deepEqual(
    native.calls.filter((call) => call.command === 'mark_desktop_notifications_read').map((call) => call.args.key),
    [notice.key, null]
  )
  assert.ok(native.calls.some((call) => call.command === 'clear_desktop_notifications'))
})

test('native provider metadata is mirrored unchanged and lock clears session history', async () => {
  const providerNotice: CenterNotification = {
    ...notice,
    key: 'provider:github',
    pipelineId: null,
    runNumber: null,
    tone: 'warning',
    level: 'major_outage',
    providerIds: [1, 2, 3],
    link: { kind: 'status_page', url: 'https://www.githubstatus.com/' },
  }
  desktop.snapshot = { revision: 3, items: [providerNotice] }
  await mount()
  await waitFor(() => assert.deepEqual(useNotificationCenterStore.getState().items, [providerNotice]))
  await publish('desktop-notifications-changed', { revision: 4, items: [] })
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  await publish('desktop-notifications-changed', { revision: 3, items: [providerNotice] })
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
})
