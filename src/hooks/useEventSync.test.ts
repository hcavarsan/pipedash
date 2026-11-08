import './fixtures/notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { InfiniteQueryObserver, QueryClient, QueryClientProvider, QueryObserver } from '@tanstack/react-query'
import { act, cleanup, render } from '@testing-library/react'
import { createElement } from 'react'

import { queryKeys } from '../lib/queryKeys'
import { runHistoryQuery } from '../lib/runHistoryQuery'
import { wsClient } from '../services/websocket'
import { pipelineActionsFixture } from '../testing/presentation'
import type { Pipeline } from '../types/pipelines'

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
  actions: pipelineActionsFixture(),
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

test('a history invalidation refreshes paged and infinite lists only for its pipeline', async () => {
  let runs = [3, 2, 1]
  let unrelatedFetches = 0
  const page = (number: number) => ({
    runs: runs.slice((number - 1) * 2, number * 2).map((run_number) => ({ run_number })),
    total_count: runs.length,
  })
  const paged = new QueryObserver(client, {
    queryKey: queryKeys.runs.list(pipeline.id, runHistoryQuery(2, 2)),
    queryFn: async () => page(2),
  })
  const infinite = new InfiniteQueryObserver(client, {
    queryKey: queryKeys.runs.infinite(pipeline.id, runHistoryQuery(1, 2)),
    queryFn: async ({ pageParam }) => page(pageParam),
    initialPageParam: 1,
    getNextPageParam: (_lastPage, _pages, lastPageParam) => (lastPageParam < 2 ? lastPageParam + 1 : undefined),
  })
  const unrelated = new QueryObserver(client, {
    queryKey: queryKeys.runs.list('other-pipeline', runHistoryQuery(1, 2)),
    queryFn: async () => {
      unrelatedFetches += 1

      return { runs: [{ run_number: 99 }], total_count: 1 }
    },
  })
  const inactiveKey = queryKeys.runs.list(pipeline.id, runHistoryQuery(1, 20))
  client.setQueryData(inactiveKey, page(1))
  const stops = [paged, infinite, unrelated].map((observer) => observer.subscribe(() => undefined))

  try {
    await Promise.all([paged.refetch(), infinite.refetch(), unrelated.refetch()])
    await infinite.fetchNextPage()
    assert.deepEqual(paged.getCurrentResult().data?.runs, [{ run_number: 1 }])
    assert.deepEqual(
      infinite.getCurrentResult().data?.pages.map((entry) => entry.runs),
      [[{ run_number: 3 }, { run_number: 2 }], [{ run_number: 1 }]]
    )
    mount()
    const socket = openSocket()
    await flush()
    runs = [4, 3, 2, 1]
    socket.emit('run-history-cache-invalidated', { pipeline_id: pipeline.id })
    await flush()

    assert.deepEqual(paged.getCurrentResult().data, page(2))
    assert.deepEqual(infinite.getCurrentResult().data?.pages, [page(1), page(2)])
    assert.equal(unrelatedFetches, 1)
    assert.deepEqual(unrelated.getCurrentResult().data, { runs: [{ run_number: 99 }], total_count: 1 })
    assert.equal(client.getQueryState(inactiveKey)?.isInvalidated, true)
  } finally {
    for (const stop of stops) {
      stop()
    }
  }
})

test('a history invalidation without a pipeline expires all lists but keeps run details', async () => {
  const histories = [
    queryKeys.runs.list(pipeline.id, runHistoryQuery(1, 20)),
    queryKeys.runs.infinite('other-pipeline', runHistoryQuery(1, 20)),
  ]
  const detailKey = queryKeys.runs.detail(pipeline.id, 1)
  for (const key of [...histories, detailKey]) {
    client.setQueryData(key, { run_number: 1 })
  }
  mount()
  const socket = openSocket()
  await flush()
  socket.emit('run-history-cache-invalidated', {})
  await flush()

  for (const key of histories) {
    assert.equal(client.getQueryState(key)?.isInvalidated, true)
  }
  assert.equal(client.getQueryState(detailKey)?.isInvalidated, false)
})
