import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { PipelineRun } from '../types'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService, tauriService } = await import('../services')
const { useRunDetails } = await import('./useRunDetailsQuery')

const wasServer = environmentManager.isServer()
environmentManager.setIsServer(() => false)
notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

const run = (status: PipelineRun['status']): PipelineRun => ({
  id: 'r',
  pipeline_id: 'p1',
  run_number: 3,
  status,
  started_at: '2026-01-01T00:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: null,
  actor: null,
})

beforeEach(() => {
  client.clear()
})
afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  environmentManager.setIsServer(() => wasServer)
  await GlobalRegistrator.unregister()
})

async function tick(context: TestContext, ms: number) {
  await act(async () => {
    context.mock.timers.tick(ms)
    const { promise, resolve } = Promise.withResolvers<void>()
    setImmediate(resolve)
    await promise
  })
}

function setup(context: TestContext, status: () => PipelineRun['status']) {
  context.mock.timers.enable({ apis: ['setTimeout', 'setInterval'] })
  const fetchDetails = context.mock.fn(async () => run(status()))
  context.mock.method(apiService, 'getWorkflowRunDetails', fetchDetails)
  context.mock.method(tauriService, 'getWorkflowRunDetails', fetchDetails)

  return fetchDetails
}

test('cancellation watch refetches every second up to ten times, then falls back to the normal interval', async (context) => {
  const fetchDetails = setup(context, () => 'running')
  renderHook(() => useRunDetails('p1', 3, true, true), { wrapper })
  await tick(context, 0)
  assert.equal(fetchDetails.mock.callCount(), 1)

  for (let index = 0; index < 10; index += 1) {
    await tick(context, 1000)
  }
  assert.equal(fetchDetails.mock.callCount(), 11)

  await tick(context, 5000)
  assert.equal(fetchDetails.mock.callCount(), 11)

  await tick(context, 5000)
  assert.equal(fetchDetails.mock.callCount(), 12)
})

test('without the watch a running run is polled only every ten seconds', async (context) => {
  const fetchDetails = setup(context, () => 'running')
  renderHook(() => useRunDetails('p1', 3, true), { wrapper })
  await tick(context, 0)

  await tick(context, 9000)
  assert.equal(fetchDetails.mock.callCount(), 1)

  await tick(context, 1000)
  assert.equal(fetchDetails.mock.callCount(), 2)
})

test('the cancellation watch stops once the run is terminal', async (context) => {
  let status: PipelineRun['status'] = 'running'
  const fetchDetails = setup(context, () => status)
  renderHook(() => useRunDetails('p1', 3, true, true), { wrapper })
  await tick(context, 0)

  status = 'cancelled'
  await tick(context, 1000)
  const settled = fetchDetails.mock.callCount()

  await tick(context, 30_000)
  assert.equal(fetchDetails.mock.callCount(), settled)
})
