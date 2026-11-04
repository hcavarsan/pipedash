import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'

import { useNotificationPreferencesSync } from '../../features/notifications/queries/useNotificationPreferencesSync'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { DEFAULT_NOTIFICATION_PREFERENCES, pendingWatches, useNotificationStore } from '../../stores/notificationStore'
import { useNotificationEvents } from '../useNotificationEvents'

import { callbacks, delivered, listeners, native, permission } from './notificationEnvironment'
import { failedNotification, outageNotification } from './notificationPayloads'
import { defineRegistrationScenarios } from './notificationRegistrationScenarios'

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const watch = { pipeline_id: failedNotification.pipeline_id ?? '', since: '2026-10-26T11:00:00.000Z' }

function Screen() {
  useNotificationEvents()
  useNotificationPreferencesSync()
  return null
}

async function mount(notificationsEnabled: boolean) {
  useNotificationStore.setState({ notificationsEnabled })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Screen)))
  })
  await waitFor(() => assert.ok(listeners.has('notification')))
}

async function publish(name: string, payload: unknown) {
  await act(async () => {
    const id = listeners.get(name)
    if (id !== undefined) {
      callbacks.get(id)?.({ event: name, id, payload })
    }
  })
}

function setOnline(online: boolean) {
  Object.defineProperty(navigator, 'onLine', { configurable: true, value: online })
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  useNotificationCenterStore.setState({ items: [], selection: null })
  useNotificationStore.setState({ notificationsEnabled: false, preferences: DEFAULT_NOTIFICATION_PREFERENCES })
  delivered.length = 0
  native.calls.length = 0
  native.sent.length = 0
  native.fired = []
  permission.granted = true
  setOnline(true)
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

defineRegistrationScenarios(client, Screen)

test('one flag delivers a failure to the desktop and the center', async () => {
  await mount(true)
  await publish('notification', failedNotification)
  const [item] = useNotificationCenterStore.getState().items
  assert.ok(item)
  assert.equal(item.pipelineId, failedNotification.pipeline_id)
  assert.deepEqual(item.providerIds, [])
  assert.equal(item.at, Date.parse(failedNotification.at))
  assert.equal(item.level, null)
  assert.equal(item.link, null)
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.deepEqual(delivered, [
    {
      title: failedNotification.title,
      body: failedNotification.body,
      target: { key: failedNotification.key, pipelineId: failedNotification.pipeline_id },
    },
  ])
})

test('the same flag delivers a provider outage with its level and link', async () => {
  await mount(true)
  await publish('notification', outageNotification)
  const [item] = useNotificationCenterStore.getState().items
  assert.ok(item)
  assert.equal(item.key, outageNotification.key)
  assert.deepEqual(item.providerIds, [1])
  assert.equal(item.pipelineId, null)
  assert.equal(item.level, 'major_outage')
  assert.deepEqual(item.link, outageNotification.link)
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.deepEqual(delivered[0]?.target, { key: outageNotification.key })
})

test('with notifications off failures and outages stay in the center only', async () => {
  await mount(false)
  await publish('notification', failedNotification)
  await publish('notification', outageNotification)
  assert.equal(useNotificationCenterStore.getState().items.length, 2)
  assert.deepEqual(delivered, [])
})

test('a denied permission keeps the in-app failure', async () => {
  await mount(true)
  permission.granted = false
  await publish('notification', failedNotification)
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, failedNotification.pipeline_id)
  assert.deepEqual(delivered, [])
})

test('nothing is shown without a notification event', async () => {
  await mount(true)
  await publish('pipelines-failed', { pipelines: [] })
  await publish('provider-health-alert', {})
  assert.equal(listeners.has('pipelines-failed'), false)
  assert.equal(listeners.has('provider-health-alert'), false)
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  assert.deepEqual(delivered, [])
})

test('a fired one-time request is marked used and stops being pending', async () => {
  useNotificationStore.setState({ preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [watch] } })
  await mount(true)
  const fired = { ...watch, fired_at: '2026-10-26T12:01:00.000Z' }
  await publish('notification', { ...failedNotification, fired })
  const { preferences } = useNotificationStore.getState()
  assert.deepEqual(preferences.watch_once, [fired])
  assert.deepEqual(pendingWatches(preferences), [])
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
})

test('offline, an outage notice without a link is skipped and everything else still arrives', async () => {
  await mount(true)
  setOnline(false)
  await publish('notification', { ...outageNotification, link: undefined })
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
  assert.deepEqual(delivered, [])

  await publish('notification', { ...outageNotification, kind: 'recovered', tone: 'success', link: undefined })
  assert.deepEqual(useNotificationCenterStore.getState().items, [])

  await publish('notification', outageNotification)
  await publish('notification', failedNotification)
  assert.deepEqual(
    useNotificationCenterStore
      .getState()
      .items.map((item) => item.key)
      .sort(),
    [failedNotification.key, outageNotification.key].sort()
  )
})

test('the desktop sends its preferences on mount and applies the used requests Rust reports', async () => {
  const fired = { ...watch, fired_at: '2026-10-26T12:01:00.000Z' }
  native.fired = [fired]
  const preferences = { ...DEFAULT_NOTIFICATION_PREFERENCES, watch_once: [watch] }
  useNotificationStore.setState({ preferences })
  await mount(true)
  await waitFor(() => assert.deepEqual(useNotificationStore.getState().preferences.watch_once, [fired]))
  assert.deepEqual(native.sent[0], preferences)
})

test('the desktop keeps its saved flag and never calls the iOS commands', async () => {
  await mount(true)
  assert.equal(useNotificationStore.getState().notificationsEnabled, true)
  assert.deepEqual(native.calls, [])
})
