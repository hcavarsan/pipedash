import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider, type UseMutationResult } from '@tanstack/react-query'
import { act, cleanup, render, renderHook, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import {
  desktopNotificationsKey,
  useNotificationActions,
  useNotificationItems,
} from '../../features/notifications/queries/useDesktopNotifications'
import {
  useLocalNotificationSettings,
  useLocalNotificationsLifecycle,
  useSetLocalNotifications,
} from '../../features/notifications/queries/useLocalNotificationQueries'
import { useNotificationPreferencesSync } from '../../features/notifications/queries/useNotificationPreferencesSync'
import { notifications } from '../../lib/notifications'
import { queryKeys } from '../../lib/queryKeys'
import { setProviderPipelines } from '../../queries/usePipelinesQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } from '../../stores/notificationStore'
import { installNotificationCenterDouble } from '../../testing/notificationCenter'
import { pipelineActionsFixture } from '../../testing/presentation'
import type { LocalNotificationSettings } from '../../types/notifications'
import type { Pipeline } from '../../types/pipelines'
import { showSystemNotification } from '../../utils/systemNotifications'
import { useNotificationEvents } from '../useNotificationEvents'

import { cachedPermission, callbacks, delivered, listeners, native, permission } from './notificationEnvironment'
import { failedNotification, outageNotification } from './notificationPayloads'
import { defineRegistrationScenarios } from './notificationRegistrationScenarios'

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const pipeline: Pipeline = {
  id: 'github__1__owner/repo/build.yml',
  provider_id: 1,
  provider_type: 'github',
  name: 'Build',
  repository: 'owner/repo',
  branch: 'main',
  workflow_file: 'build.yml',
  status: 'failed',
  last_run: '2026-10-26T12:00:00Z',
  last_updated: '2026-10-26T12:01:00Z',
  actions: pipelineActionsFixture(),
}
let mutation: UseMutationResult<LocalNotificationSettings, Error, boolean> | undefined
function Hooks() {
  useNotificationEvents()
  return null
}

function Lifecycle() {
  useLocalNotificationsLifecycle()
  useNotificationPreferencesSync()
  useLocalNotificationSettings()
  mutation = useSetLocalNotifications()
  return null
}

async function mount() {
  useNotificationStore.setState({ notificationsEnabled: true })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Hooks)))
  })
  await waitFor(() => assert.ok(listeners.has('notification')))
}

async function mountLifecycle() {
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Lifecycle)))
  })
}

async function publish(name: string, payload: unknown) {
  await act(async () => {
    const id = listeners.get(name)
    if (id !== undefined) {
      callbacks.get(id)?.({ event: name, id, payload })
    }
  })
}

installNotificationCenterDouble()

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  useNotificationCenterStore.setState({ items: [], revision: 0, unread: 0, selection: null })
  useNotificationStore.setState({ notificationsEnabled: false, preferences: DEFAULT_NOTIFICATION_PREFERENCES })
  delivered.length = 0
  mutation = undefined
  native.settings = { enabled: false, available: true, reason: null }
  native.calls.length = 0
  native.sent.length = 0
  native.failure = null
  native.fired = []
  permission.granted = true
  cachedPermission.permission = 'default'
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

defineRegistrationScenarios(client, Hooks)

test('a failure reaches the center and leaves iOS delivery to Rust', async () => {
  await mount()
  await publish('notification', failedNotification)
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, failedNotification.pipeline_id)
  assert.deepEqual(delivered, [])
})

test('a failure still reaches the center when notifications are off', async () => {
  await mount()
  useNotificationStore.setState({ notificationsEnabled: false })
  await publish('notification', failedNotification)
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, failedNotification.pipeline_id)
  assert.deepEqual(delivered, [])
})

test('denied iOS permission does not discard the in-app failure', async () => {
  await mount()
  permission.granted = false
  await publish('notification', failedNotification)
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, failedNotification.pipeline_id)
  assert.deepEqual(delivered, [])
})

test('a provider outage reaches the center without a webview delivery or Remote Push', async () => {
  await mount()
  await publish('notification', outageNotification)
  assert.equal(useNotificationCenterStore.getState().items[0]?.key, outageNotification.key)
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
  assert.deepEqual(delivered, [])
})

test('offline, a provider recovery without a link is dropped but a pipeline that passes again still shows', async () => {
  Object.defineProperty(navigator, 'onLine', { configurable: true, value: false })
  try {
    await mount()
    const { link: _link, ...unlinked } = outageNotification
    await publish('notification', { ...unlinked, id: 21, kind: 'recovered', tone: 'success' })
    await publish('notification', {
      ...failedNotification,
      id: 22,
      kind: 'recovered',
      tone: 'success',
      title: 'Build passed again',
    })
    assert.deepEqual(
      useNotificationCenterStore.getState().items.map((item) => item.title),
      ['Build passed again']
    )
  } finally {
    Reflect.deleteProperty(navigator, 'onLine')
  }
})

test('a degraded notice keeps the title and reason Rust wrote', async () => {
  await mount()
  const body = 'owner/repo · github · Pod apps/web-7f9c: CrashLoopBackOff'
  await publish('notification', {
    ...failedNotification,
    kind: 'degraded',
    title: 'Build is degraded',
    body,
  })
  const [item] = useNotificationCenterStore.getState().items
  assert.equal(item?.title, 'Build is degraded')
  assert.equal(item?.body, body)
})

test('pipeline lists changing, loading or going overdue raise nothing without a notification event', async () => {
  await mount()
  setProviderPipelines(client, pipeline.provider_id, [{ ...pipeline, status: 'running', overdue_run: null }])
  setProviderPipelines(client, pipeline.provider_id, [pipeline])
  setProviderPipelines(client, pipeline.provider_id, [{ ...pipeline, status: 'running', overdue_run: 'run-42' }])
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  assert.deepEqual(delivered, [])
})

test('a permission grant in iOS Settings takes effect without reloading the webview', async () => {
  cachedPermission.permission = 'denied'
  permission.granted = true
  await act(async () => {
    showSystemNotification('Outage', 'Provider unavailable', { key: 'provider:1', providerId: 1 })
  })
  await waitFor(() => assert.equal(delivered.length, 1))
})

test('revoking iOS permission overrides the webview permission cache', async () => {
  cachedPermission.permission = 'granted'
  permission.granted = false
  await act(async () => {
    showSystemNotification('Outage', 'Provider unavailable', { key: 'provider:1', providerId: 1 })
  })
  assert.deepEqual(delivered, [])
})

test('a legacy enabled preference never provisions background access and the query follows the backend', async () => {
  useNotificationStore.setState({ notificationsEnabled: true })
  await mountLifecycle()
  await waitFor(() =>
    assert.equal(client.getQueryData<{ enabled: boolean }>(queryKeys.localNotifications.settings())?.enabled, false)
  )
  assert.deepEqual(
    native.calls.map((call) => call.command),
    ['get_local_notification_settings']
  )
})

test('a one-time request the device already used is marked used when the app syncs, and the others stay pending', async () => {
  const used = { pipeline_id: pipeline.id, since: '2026-11-04T10:00:00.000Z' }
  const waiting = { pipeline_id: 'github__1__owner/repo/deploy.yml', since: '2026-11-04T10:05:00.000Z' }
  const firedAt = '2026-11-04T10:30:00.000Z'
  native.fired = [{ ...used, fired_at: firedAt }]
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [used, waiting] } })

  await mountLifecycle()

  await waitFor(() =>
    assert.deepEqual(useNotificationStore.getState().preferences.watch_once, [{ ...used, fired_at: firedAt }, waiting])
  )
  assert.deepEqual(
    useNotificationStore.getState().preferences.watch_once.filter((item) => !item.fired_at),
    [waiting]
  )
})

test('a backend that already has notifications on reports them on', async () => {
  native.settings = { enabled: true, available: true, reason: null }
  await mountLifecycle()
  await waitFor(() =>
    assert.equal(client.getQueryData<{ enabled: boolean }>(queryKeys.localNotifications.settings())?.enabled, true)
  )
})

test('returning to the foreground adopts a change made outside the app', async () => {
  native.settings = { enabled: true, available: true, reason: null }
  await mountLifecycle()
  await waitFor(() =>
    assert.equal(client.getQueryData<{ available: boolean }>(queryKeys.localNotifications.settings())?.available, true)
  )
  native.settings = { enabled: true, available: false, reason: 'Allow notifications in Settings.' }
  await act(async () => {
    window.dispatchEvent(new Event('focus'))
  })
  await waitFor(() =>
    assert.deepEqual(client.getQueryData(queryKeys.localNotifications.settings()), {
      enabled: true,
      available: false,
      reason: 'Allow notifications in Settings.',
    })
  )
})

test('enabling sends the explicit command and records the backend answer', async () => {
  await mountLifecycle()
  await waitFor(() => assert.ok(client.getQueryData(queryKeys.localNotifications.settings())))
  await act(async () => {
    await mutation?.mutateAsync(true)
  })
  assert.deepEqual(native.calls.find((call) => call.command === 'set_local_notifications_enabled')?.args, {
    enabled: true,
  })
  assert.equal(client.getQueryData<{ enabled: boolean }>(queryKeys.localNotifications.settings())?.enabled, true)
})

test('a failed enable reports the error and never turns notifications on', async (context) => {
  const shown = context.mock.method(notifications, 'show', () => 'notification')
  native.failure = new Error('Keychain unavailable')
  await mountLifecycle()
  await waitFor(() => assert.ok(client.getQueryData(queryKeys.localNotifications.settings())))
  await act(async () => {
    await mutation?.mutateAsync(true).catch(() => undefined)
  })
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(client.getQueryData<{ enabled: boolean }>(queryKeys.localNotifications.settings())?.enabled, false)
})

test('shared notification hooks keep mobile history and read actions local', async () => {
  await mount()
  await publish('notification', failedNotification)
  const { result } = renderHook(() => ({ items: useNotificationItems().items, ...useNotificationActions() }), {
    wrapper: ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children),
  })
  const first = result.current.items[0]
  assert.ok(first)
  await act(async () => result.current.markRead(first.key))
  await waitFor(() => assert.equal(result.current.items[0]?.read, true))
  await act(async () => useNotificationCenterStore.getState().push({ ...first, key: 'second' }))
  await act(async () => result.current.markAllRead())
  await waitFor(() => assert.ok(result.current.items.every((item) => item.read)))
  act(() => result.current.open({ type: 'notification', key: first.key }))
  assert.deepEqual(useNotificationCenterStore.getState().selection, { type: 'notification', key: first.key })
  await act(async () => result.current.clear())
  await waitFor(() => assert.deepEqual(result.current.items, []))
  assert.equal(client.getQueryData(desktopNotificationsKey), undefined)
  assert.equal(
    native.calls.some((call) => call.command.includes('desktop_notifications')),
    false
  )
})
