import './notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import { queryKeys } from '../../lib/queryKeys'
import { setProviderPipelines } from '../../queries/usePipelinesQueries'
import { wsClient } from '../../services/websocket'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import { useNotificationStore } from '../../stores/notificationStore'
import type { Pipeline } from '../../types/domain'
import { useEventSync } from '../useEventSync'
import { useFailureNotifications } from '../useFailureNotifications'

import { delivered, Socket } from './notificationWebEnvironment'

const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
const baseline: Pipeline = {
  id: 'github__1__owner/repo/build.yml',
  provider_id: 1,
  provider_type: 'github',
  name: 'Build',
  repository: 'owner/repo',
  branch: 'main',
  workflow_file: 'build.yml',
  status: 'running',
  last_run: '2026-10-26T12:00:00Z',
  last_updated: '2026-10-26T12:01:00Z',
}
let backend: Pipeline[] = []
let failNext = false
let gate: PromiseWithResolvers<void> | null = null

function Screen() {
  useFailureNotifications()
  useEventSync()
  useQuery({
    queryKey: queryKeys.pipelines.list(),
    queryFn: async () => {
      await gate?.promise
      if (failNext) {
        throw new Error('backend unavailable')
      }

      return backend
    },
  })
  return null
}

async function mount(initial: Pipeline) {
  backend = [initial]
  useNotificationStore.setState({ notificationsEnabled: true })
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Screen)))
  })
  await waitFor(() => assert.deepEqual(client.getQueryData(queryKeys.pipelines.list()), backend))
  const socket = Socket.instances.at(-1)
  assert.ok(socket)
  socket.open()
  return socket
}

afterEach(async () => {
  await act(async () => cleanup())
  wsClient.dispose()
  client.clear()
  useNotificationCenterStore.setState({ items: [], selection: null })
  Socket.instances.length = 0
  delivered.length = 0
  failNext = false
  gate = null
})

after(() => GlobalRegistrator.unregister())

test('reconnection refetch recovers a failure missed while WebSocket was disconnected', async () => {
  await mount(baseline)
  wsClient.disconnect()
  backend = [{ ...baseline, status: 'failed' }]
  wsClient.manualReconnect()
  Socket.instances.at(-1)?.open()
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, baseline.id)
})

test('reconnection notices a newer failed run on an already-failed pipeline', async () => {
  await mount({ ...baseline, status: 'failed' })
  assert.deepEqual(delivered, [])
  wsClient.disconnect()
  backend = [{ ...baseline, status: 'failed', last_run: '2026-10-26T12:05:00Z' }]
  wsClient.manualReconnect()
  Socket.instances.at(-1)?.open()
  await waitFor(() => assert.equal(delivered.length, 1))
})

test('live updates and refetches notify once regardless of failure event ordering', async () => {
  const socket = await mount(baseline)
  for (const eventFirst of [true, false]) {
    const failed: Pipeline = {
      ...baseline,
      status: 'failed',
      last_run: eventFirst ? '2026-10-26T12:05:00Z' : '2026-10-26T12:10:00Z',
    }
    backend = [failed]
    await act(async () => {
      if (eventFirst) {
        socket.emit('pipelines-failed', { pipelines: [failed] })
      }
      socket.emit('pipelines-updated', { pipelines: [failed], providerId: 1, timestamp: Date.now() })
      if (!eventFirst) {
        socket.emit('pipelines-failed', { pipelines: [failed] })
      }
    })
    await act(async () => {
      await client.invalidateQueries({ queryKey: queryKeys.pipelines.list() })
    })
  }
  await waitFor(() => assert.equal(delivered.length, 2))
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
})

test('loading and refreshing the same failed run does not replay a historical alert', async () => {
  await mount({ ...baseline, status: 'failed' })
  await act(async () => {
    setProviderPipelines(client, 1, backend)
    await client.invalidateQueries({ queryKey: queryKeys.pipelines.list() })
  })
  assert.deepEqual(delivered, [])
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
})

test('reconnection recovery stays in the center when notifications are off', async () => {
  await mount(baseline)
  useNotificationStore.setState({ notificationsEnabled: false })
  wsClient.disconnect()
  backend = [{ ...baseline, status: 'failed' }]
  wsClient.manualReconnect()
  Socket.instances.at(-1)?.open()
  await waitFor(() => assert.equal(useNotificationCenterStore.getState().items[0]?.pipelineId, baseline.id))
  assert.deepEqual(delivered, [])
})

test('a failed reconnect refetch stays silent and the next successful one still recovers the failure', async (context) => {
  context.mock.method(console, 'error', () => undefined)
  await mount(baseline)
  wsClient.disconnect()
  backend = [{ ...baseline, status: 'failed' }]
  failNext = true
  wsClient.manualReconnect()
  Socket.instances.at(-1)?.open()
  await waitFor(() => assert.equal(client.getQueryState(queryKeys.pipelines.list())?.status, 'error'))
  assert.deepEqual(delivered, [])

  failNext = false
  wsClient.disconnect()
  wsClient.manualReconnect()
  Socket.instances.at(-1)?.open()
  await waitFor(() => assert.equal(delivered.length, 1))
  assert.equal(useNotificationCenterStore.getState().items.length, 1)
})

test('a slow initial list does not announce an already failed pipeline when it arrives', async () => {
  backend = [{ ...baseline, status: 'failed' }]
  useNotificationStore.setState({ notificationsEnabled: true })
  gate = Promise.withResolvers<void>()
  await act(async () => {
    render(createElement(QueryClientProvider, { client }, createElement(Screen)))
  })
  assert.deepEqual(delivered, [])

  gate.resolve()
  await waitFor(() => assert.deepEqual(client.getQueryData(queryKeys.pipelines.list()), backend))
  assert.deepEqual(delivered, [])
  assert.deepEqual(useNotificationCenterStore.getState().items, [])
})
