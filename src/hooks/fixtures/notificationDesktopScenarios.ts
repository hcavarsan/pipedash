import './notificationEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'

import { useLocalNotificationsLifecycle } from '../../queries/useLocalNotificationQueries'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { useNotificationStore } from '../../stores/notificationStore'
import type { Pipeline, ProviderHealthAlert } from '../../types'
import { useFailureNotifications } from '../useFailureNotifications'
import { useProviderHealthAlerts } from '../useProviderHealthAlerts'

import { callbacks, delivered, listeners, native, permission } from './notificationEnvironment'

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

function Screen() {
  useFailureNotifications()
  useProviderHealthAlerts()
  useLocalNotificationsLifecycle()
  return null
}

async function mount(notificationsEnabled: boolean) {
  useNotificationStore.setState({ notificationsEnabled })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Screen)))
  })
  await waitFor(() => assert.ok(listeners.has('provider-health-alert') && listeners.has('pipelines-failed')))
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
  native.calls.length = 0
  permission.granted = true
  listeners.clear()
  callbacks.clear()
})

after(() => GlobalRegistrator.unregister())

test('one flag delivers a backend-confirmed failure to the desktop and the center', async () => {
  await mount(true)
  await publish('pipelines-failed', { pipelines: [pipeline] })
  const [item] = useNotificationCenterStore.getState().items
  assert.equal(item?.pipelineId, pipeline.id)
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.deepEqual([delivered[0].title, delivered[0].body], [item.title, item.body])
  assert.equal(item.body, 'owner/repo · github')
})

test('the same flag delivers a provider outage', async () => {
  await mount(true)
  await publish('provider-health-alert', outage)
  const [item] = useNotificationCenterStore.getState().items
  assert.equal(item?.key, outage.notification_key)
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.deepEqual([delivered[0].title, delivered[0].body], [item.title, item.body])
})

test('with notifications off failures and outages stay in the center only', async () => {
  await mount(false)
  await publish('pipelines-failed', { pipelines: [pipeline] })
  await publish('provider-health-alert', outage)
  assert.equal(useNotificationCenterStore.getState().items.length, 2)
  assert.deepEqual(delivered, [])
})

test('a denied permission keeps the in-app failure', async () => {
  await mount(true)
  permission.granted = false
  await publish('pipelines-failed', { pipelines: [pipeline] })
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, pipeline.id)
  assert.deepEqual(delivered, [])
})

test('the desktop keeps its saved flag and never calls the iOS commands', async () => {
  await mount(true)
  assert.equal(useNotificationStore.getState().notificationsEnabled, true)
  assert.deepEqual(native.calls, [])
})
