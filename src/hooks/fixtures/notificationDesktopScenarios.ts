import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, renderHook, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'

import {
  desktopNotificationsKey,
  refreshDesktopNotifications,
  useDesktopNotifications,
  useNotificationActions,
  useNotificationItems,
} from '../../features/notifications/queries/useDesktopNotifications'
import { useLocalNotificationsLifecycle } from '../../features/notifications/queries/useLocalNotificationQueries'
import { useDesktopLifecycle, useDesktopStatus } from '../../features/settings/queries/useDesktopQueries'
import { useFactoryReset } from '../../features/settings/queries/usePlatformQueries'
import { notifications } from '../../lib/notifications'
import { type DesktopNotificationSnapshot, desktopService } from '../../services/desktop'
import { service } from '../../services/service'
import { type CenterNotification, useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { useNotificationStore } from '../../stores/notificationStore'
import type { DesktopStatus } from '../../types/domain'
import { useFailureNotifications } from '../useFailureNotifications'
import { useProviderHealthAlerts } from '../useProviderHealthAlerts'

import { callbacks, delivered, desktop, listeners, native, permission } from './notificationEnvironment'
import { defineRegistrationScenarios } from './notificationRegistrationScenarios'

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
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
  const items = useNotificationItems()
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
  useNotificationCenterStore.setState({ items: [], selection: null })
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
  assert.deepEqual(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items, [notice])
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  await publish('desktop-notifications-changed', { revision: 7, items: [] })
  assert.equal(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items.length, 1)
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
  await waitFor(() =>
    assert.equal(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items.length, 2)
  )
  const { result } = renderHook(useNotificationActions, { wrapper })
  act(() => result.current.open({ type: 'notification', key: notice.key }))
  await waitFor(() =>
    assert.equal(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items[0]?.read, true)
  )
  assert.equal(useNotificationCenterStore.getState().selection?.type, 'notification')
  act(() => result.current.markAllRead())
  await waitFor(() =>
    assert.ok(
      client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items.every((item) => item.read)
    )
  )
  act(() => result.current.clear())
  await waitFor(() =>
    assert.deepEqual(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items, [])
  )
  assert.deepEqual(
    native.calls.filter((call) => call.command === 'mark_desktop_notifications_read').map((call) => call.args.key),
    [notice.key, null]
  )
  assert.ok(native.calls.some((call) => call.command === 'clear_desktop_notifications'))
})

test('native provider metadata stays unchanged and lock clears session history', async () => {
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
  await waitFor(() =>
    assert.deepEqual(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items, [providerNotice])
  )
  await publish('desktop-notifications-changed', { revision: 4, items: [] })
  assert.deepEqual(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items, [])
  await publish('desktop-notifications-changed', { revision: 3, items: [providerNotice] })
  assert.deepEqual(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items, [])
})

test('a stale command reply cannot overwrite a newer native event', async (context) => {
  desktop.snapshot = { revision: 1, items: [notice] }
  await mount()
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
  const reply = Promise.withResolvers<DesktopNotificationSnapshot>()
  const mark = context.mock.method(desktopService, 'markNotificationsRead', () => reply.promise)
  const { result } = renderHook(useNotificationActions, { wrapper })
  act(() => result.current.markRead(notice.key))
  await waitFor(() => assert.equal(mark.mock.callCount(), 1))
  assert.equal(client.getQueryData<DesktopNotificationSnapshot>(desktopNotificationsKey)?.items[0]?.read, false)
  const newer = { revision: 3, items: [notice, { ...notice, key: 'newer' }] }
  await publish('desktop-notifications-changed', newer)
  await act(async () => reply.resolve({ revision: 2, items: [{ ...notice, read: true }] }))
  assert.deepEqual(client.getQueryData(desktopNotificationsKey), newer)
})

test('unmount removes cached history and ignores an in-flight fetch, mutation and event', async (context) => {
  const read = Promise.withResolvers<DesktopNotificationSnapshot>()
  desktop.readGate = read.promise
  await mount()
  await publish('desktop-notifications-changed', { revision: 1, items: [notice] })
  const reply = Promise.withResolvers<DesktopNotificationSnapshot>()
  const mark = context.mock.method(desktopService, 'markNotificationsRead', () => reply.promise)
  const { result } = renderHook(useNotificationActions, { wrapper })
  act(() => result.current.open({ type: 'notification', key: notice.key }))
  await waitFor(() => assert.equal(mark.mock.callCount(), 1))
  const id = listeners.get('desktop-notifications-changed')
  assert.notEqual(id, undefined)
  const callback = callbacks.get(id ?? -1)
  await act(async () => cleanup())
  assert.equal(client.getQueryData(desktopNotificationsKey), undefined)
  assert.equal(useNotificationCenterStore.getState().selection, null)
  await act(async () => {
    callback?.({ payload: { revision: 9, items: [notice] } })
    read.resolve({ revision: 2, items: [notice] })
    reply.resolve({ revision: 3, items: [{ ...notice, read: true }] })
  })
  assert.equal(client.getQueryData(desktopNotificationsKey), undefined)
  desktop.readGate = null
  desktop.snapshot = { revision: 10, items: [] }
  await mount()
  await waitFor(() => assert.deepEqual(client.getQueryData(desktopNotificationsKey), desktop.snapshot))
})

test('factory reset reloads the snapshot and rejects replies from the removed cache', async (context) => {
  desktop.snapshot = { revision: 1, items: [notice] }
  await mount()
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
  const reply = Promise.withResolvers<DesktopNotificationSnapshot>()
  const clear = context.mock.method(desktopService, 'clearNotifications', () => reply.promise)
  const { result: actions } = renderHook(useNotificationActions, { wrapper })
  act(() => actions.current.clear())
  await waitFor(() => assert.equal(clear.mock.callCount(), 1))
  context.mock.method(notifications, 'show', () => 'reset-complete')
  context.mock.method(service, 'factoryReset', async () => {
    desktop.snapshot = { revision: 3, items: [] }
    return { providers_removed: 0, caches_cleared: true, tokens_cleared: true, metrics_cleared: true }
  })
  const { result: reset } = renderHook(useFactoryReset, { wrapper })
  await act(async () => {
    await reset.current.mutateAsync()
  })
  await waitFor(() => assert.deepEqual(client.getQueryData(desktopNotificationsKey), desktop.snapshot))
  await act(async () => reply.resolve({ revision: 99, items: [notice] }))
  assert.deepEqual(client.getQueryData(desktopNotificationsKey), { revision: 3, items: [] })
  const latest = { revision: 4, items: [{ ...notice, key: 'after-reset' }] }
  await publish('desktop-notifications-changed', latest)
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
  assert.deepEqual(client.getQueryData(desktopNotificationsKey), latest)
})

test('failed reads stay in the query error path and a later refresh recovers', async (context) => {
  const warn = context.mock.method(console, 'warn', () => undefined)
  const read = Promise.withResolvers<DesktopNotificationSnapshot>()
  desktop.readGate = read.promise
  await mount()
  await waitFor(() => assert.ok(native.calls.some((call) => call.command === 'get_desktop_notifications')))
  await act(async () => read.reject(new Error('History unavailable')))
  assert.equal(client.getQueryState(desktopNotificationsKey)?.status, 'error')
  assert.ok(warn.mock.callCount() > 0)
  desktop.readGate = null
  desktop.snapshot = { revision: 2, items: [notice] }
  await act(async () => {
    await refreshDesktopNotifications(client)
  })
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
})

test('a rejected mutation preserves history and displays its failure', async (context) => {
  desktop.snapshot = { revision: 1, items: [notice] }
  await mount()
  await waitFor(() => assert.ok(document.body.textContent?.includes(notice.title)))
  context.mock.method(desktopService, 'clearNotifications', async () => {
    throw new Error('History could not be cleared')
  })
  const shown = context.mock.method(notifications, 'show', () => 'clear-failure')
  const { result } = renderHook(useNotificationActions, { wrapper })
  act(() => result.current.clear())
  await waitFor(() => assert.equal(shown.mock.callCount(), 1))
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, "Couldn't update notifications")
  assert.deepEqual(client.getQueryData(desktopNotificationsKey), desktop.snapshot)
})
