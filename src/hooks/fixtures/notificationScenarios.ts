import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { notifications } from '@mantine/notifications'
import { QueryClient, QueryClientProvider, type UseMutationResult } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import {
  useLocalNotificationSettings,
  useLocalNotificationsLifecycle,
  useSetLocalNotifications,
} from '../../features/notifications/queries/useLocalNotificationQueries'
import { queryKeys } from '../../lib/queryKeys'
import { setProviderPipelines } from '../../queries/usePipelinesQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { useNotificationStore } from '../../stores/notificationStore'
import type { LocalNotificationSettings, Pipeline, ProviderHealthAlert } from '../../types/domain'
import { showSystemNotification } from '../../utils/systemNotifications'
import { useFailureNotifications } from '../useFailureNotifications'
import { useProviderHealthAlerts } from '../useProviderHealthAlerts'

import { cachedPermission, callbacks, delivered, listeners, native, permission } from './notificationEnvironment'
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
}
const outage: ProviderHealthAlert = {
  alert_id: 'alert-1',
  notification_key: 'status-page:github',
  provider_ids: [1],
  level: 'major_outage',
  title: 'GitHub Actions outage',
  body: 'Workflow runs are delayed.',
  link: { url: 'https://www.githubstatus.com/', kind: 'status_page' },
  recovered: false,
}
let mutation: UseMutationResult<LocalNotificationSettings, Error, boolean> | undefined
function Hooks() {
  useFailureNotifications()
  useProviderHealthAlerts()
  return null
}

function Lifecycle() {
  useLocalNotificationsLifecycle()
  useLocalNotificationSettings()
  mutation = useSetLocalNotifications()
  return null
}

async function mount() {
  useNotificationStore.setState({ notificationsEnabled: true })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Hooks)))
  })
  await waitFor(() => assert.ok(listeners.has('provider-health-alert')))
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

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  useNotificationCenterStore.setState({ items: [], selection: null })
  useNotificationStore.setState({ notificationsEnabled: false })
  delivered.length = 0
  mutation = undefined
  native.settings = { enabled: false, available: true, reason: null }
  native.calls.length = 0
  native.failure = null
  permission.granted = true
  cachedPermission.permission = 'default'
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

defineRegistrationScenarios(client, Hooks)

test('a backend-confirmed failure reaches the center before a pipeline list loads and leaves iOS delivery to Rust', async () => {
  await mount()
  await publish('pipelines-failed', { pipelines: [pipeline] })
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, pipeline.id)
  assert.deepEqual(delivered, [])
})

test('consecutive failed runs notify once each without duplicating cache updates', async () => {
  await mount()
  client.setQueryData(queryKeys.pipelines.list(), [pipeline])
  for (const last_run of ['2026-10-26T12:05:00Z', '2026-10-26T12:10:00Z']) {
    const failed = { ...pipeline, last_run }
    setProviderPipelines(client, pipeline.provider_id, [failed])
    await publish('pipelines-failed', { pipelines: [failed] })
    setProviderPipelines(client, pipeline.provider_id, [failed])
  }
  const { items } = useNotificationCenterStore.getState()
  assert.equal(items.length, 1)
  const [item] = items
  assert.ok(item)
  assert.equal(item.read, false)
  assert.deepEqual(delivered, [])
})

test('a running-to-failed cache update does not duplicate the backend failure alert', async () => {
  await mount()
  setProviderPipelines(client, pipeline.provider_id, [{ ...pipeline, status: 'running' }])
  setProviderPipelines(client, pipeline.provider_id, [pipeline])
  await publish('pipelines-failed', { pipelines: [pipeline] })
  const first = useNotificationCenterStore.getState().items[0]
  assert.ok(first)
  useNotificationCenterStore.getState().markRead(first.key)
  setProviderPipelines(client, pipeline.provider_id, [pipeline])
  const { items } = useNotificationCenterStore.getState()
  assert.equal(items.length, 1)
  assert.equal(items[0]?.read, true)
})

test('overdue runs still alert once when their cached observation changes', async () => {
  await mount()
  const running: Pipeline = { ...pipeline, status: 'running', overdue_run: null }
  setProviderPipelines(client, pipeline.provider_id, [running])
  const overdue = { ...running, overdue_run: 'run-42' }
  setProviderPipelines(client, pipeline.provider_id, [overdue])
  setProviderPipelines(client, pipeline.provider_id, [overdue])
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
  assert.equal(useNotificationCenterStore.getState().items[0]?.tone, 'warning')
  assert.deepEqual(delivered, [])
})

test('loading existing failed pipelines does not replay historical alerts', async () => {
  await mount()
  setProviderPipelines(client, pipeline.provider_id, [pipeline])
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  assert.deepEqual(delivered, [])
})

test('a failure still reaches the center when notifications are off', async () => {
  await mount()
  useNotificationStore.setState({ notificationsEnabled: false })
  await publish('pipelines-failed', { pipelines: [pipeline] })
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, pipeline.id)
  assert.deepEqual(delivered, [])
})

test('denied iOS permission does not discard the in-app failure', async () => {
  await mount()
  permission.granted = false
  await publish('pipelines-failed', { pipelines: [pipeline] })
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, pipeline.id)
  assert.deepEqual(delivered, [])
})

test('a provider outage reaches the center without a webview delivery or Remote Push', async () => {
  await mount()
  await publish('provider-health-alert', outage)
  assert.equal(useNotificationCenterStore.getState().items[0]?.key, outage.notification_key)
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
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
