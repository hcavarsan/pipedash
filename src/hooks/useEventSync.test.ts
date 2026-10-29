import './fixtures/notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render } from '@testing-library/react'
import { createElement } from 'react'

import { queryKeys } from '../lib/queryKeys'
import { wsClient } from '../services/websocket'
import type { Pipeline } from '../types/domain'

import { Socket } from './fixtures/notificationWebEnvironment'
import { useEventSync } from './useEventSync'

const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })

const pipeline: Pipeline = {
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

function Probe() {
  useEventSync()

  return null
}

function mount() {
  return render(createElement(QueryClientProvider, { client }, createElement(Probe)))
}

function openSocket() {
  const socket = Socket.instances.at(-1)
  assert.ok(socket)
  socket.open()

  return socket
}

const flush = () => act(async () => undefined)

afterEach(async () => {
  await act(async () => cleanup())
  wsClient.dispose()
  client.clear()
  Socket.instances.length = 0
})

after(() => GlobalRegistrator.unregister())

test('an event delivered after unmount but before registration settles is ignored', async () => {
  client.setQueryData(queryKeys.providers.list(), [])
  mount()
  const socket = openSocket()
  cleanup()
  socket.emit('providers-changed', {})
  await flush()
  assert.equal(client.getQueryState(queryKeys.providers.list())?.isInvalidated, false)
})

test('a pipelines update that finishes cancelling after unmount does not write the cache', async () => {
  mount()
  const socket = openSocket()
  await flush()
  socket.emit('pipelines-updated', { pipelines: [pipeline], providerId: 1, timestamp: 1 })
  cleanup()
  await flush()
  assert.equal(client.getQueryData(queryKeys.pipelines.list({ providerId: 1 })), undefined)
})

test('a provider status update that finishes cancelling after unmount does not write the cache', async () => {
  const summary = {
    id: 1,
    name: 'fresh',
    provider_type: 'github',
    icon: null,
    pipeline_count: 0,
    last_synced_at: null,
    last_sync_error: null,
    refresh_interval: 30,
    configured_repositories: [],
    health: { level: 'major_outage' },
  }
  client.setQueryData(queryKeys.providers.list(), [])
  mount()
  const socket = openSocket()
  await flush()
  socket.emit('provider-status-updated', { provider: summary })
  cleanup()
  await flush()
  assert.deepEqual(client.getQueryData(queryKeys.providers.list()), [])
})

test('batched invalidations scheduled before an unmount do not leak into the next mount', async () => {
  const first = { ...pipeline }
  client.setQueryData(queryKeys.pipelines.list({ providerId: 1 }), [first])
  client.setQueryData(queryKeys.pipelines.list({ providerId: 2 }), [{ ...first, provider_id: 2 }])
  mount()
  const socket = openSocket()
  await flush()
  mock.timers.enable({ apis: ['setTimeout'] })
  try {
    socket.emit('provider-added', { provider: { id: 1 } })
    cleanup()
    mount()
    await flush()
    Socket.instances.at(-1)?.emit('provider-added', { provider: { id: 2 } })
    mock.timers.tick(400)
  } finally {
    mock.timers.reset()
  }
  assert.equal(client.getQueryState(queryKeys.pipelines.list({ providerId: 2 }))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.pipelines.list({ providerId: 1 }))?.isInvalidated, false)
})
