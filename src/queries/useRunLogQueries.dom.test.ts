import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { LogDocumentMeta, RunLogDocument, RunLogTree } from '../types/logs'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider, timeoutManager } = await import(
  '@tanstack/react-query'
)
const { events } = await import('../services/service')
const { apiService } = await import('../services/api')
const { tauriService } = await import('../services/tauri')
const { useRunLogDocument, useRunLogTree } = await import('./useRunLogQueries')

interface ManualTimer {
  at: number
  every: number | null
  run: () => void
}

const MANUAL_ID_BASE = 1_000_000_000
const timers = new Map<number, ManualTimer>()
let now = 0
let nextId = MANUAL_ID_BASE

const schedule = (run: () => void, delay: number, every: number | null) => {
  nextId += 1
  timers.set(nextId, { at: now + delay, every, run })

  return nextId
}

const advance = (ms: number) => {
  const target = now + ms

  for (;;) {
    const due = [...timers.entries()].filter(([, timer]) => timer.at <= target).sort(([, a], [, b]) => a.at - b.at)[0]

    if (!due) {
      break
    }

    const [id, timer] = due

    now = timer.at
    if (timer.every === null) {
      timers.delete(id)
    } else {
      timer.at += timer.every
    }
    timer.run()
  }
  now = target
}

const realTimers = new Map<number, ReturnType<typeof setTimeout>>()

const realTimeout = (run: () => void, delay: number) => {
  nextId += 1
  const id = nextId

  realTimers.set(
    id,
    setTimeout(() => {
      realTimers.delete(id)
      run()
    }, delay)
  )

  return id
}

const cancel = (id: number | undefined) => {
  if (id === undefined || timers.delete(id)) {
    return
  }
  const real = realTimers.get(id)

  if (real !== undefined) {
    clearTimeout(real)
    realTimers.delete(id)
  }
}

const wasServer = environmentManager.isServer()
environmentManager.setIsServer(() => false)
timeoutManager.setTimeoutProvider<number>({
  setTimeout: (run, delay) => (delay > 0 ? schedule(run, delay, null) : realTimeout(run, delay)),
  clearTimeout: cancel,
  setInterval: (run, delay) => schedule(run, delay, delay),
  clearInterval: cancel,
})

const readyMeta: LogDocumentMeta = {
  node_id: '99',
  revision: 3,
  complete: true,
  truncated: false,
  line_count: 12,
  text_bytes: 120,
  first_timestamp_ms: null,
  styles: [],
  sections: [],
  error_lines: [],
  warning_lines: [],
  error_total: 0,
  warning_total: 0,
  focus_line: null,
  updated_at_ms: 0,
  stable_lines: 0,
  previous_revision: null,
}

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

beforeEach(() => {
  client.clear()
})

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  timeoutManager.setTimeoutProvider<ReturnType<typeof setTimeout>>({
    setTimeout: (run, delay) => setTimeout(run, delay),
    clearTimeout: (id) => clearTimeout(id),
    setInterval: (run, delay) => setInterval(run, delay),
    clearInterval: (id) => clearInterval(id),
  })
  environmentManager.setIsServer(() => wasServer)
  await GlobalRegistrator.unregister()
})

test('a pending job becomes ready on the 30 s heartbeat without reselection', async (context) => {
  let current: RunLogDocument = { state: 'pending' }
  const opens = context.mock.fn(async () => current)

  context.mock.method(apiService, 'openRunLog', opens)
  context.mock.method(tauriService, 'openRunLog', opens)
  context.mock.method(events, 'listen', async () => () => undefined)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result } = renderHook(() => useRunLogDocument('github__1__o/r/w', 7, '99', true), { wrapper })

  await waitFor(() => assert.equal(result.current.data?.state, 'pending'))
  assert.equal(opens.mock.callCount(), 1)

  current = { state: 'ready', ...readyMeta }
  await act(async () => advance(29_999))
  assert.equal(result.current.data?.state, 'pending')
  assert.equal(opens.mock.callCount(), 1)

  await act(async () => advance(1))
  await waitFor(() => assert.equal(result.current.data?.state, 'ready'))
  assert.equal(opens.mock.callCount(), 2)
})

test('a complete document is never refetched on a timer', async (context) => {
  context.mock.method(events, 'listen', async () => () => undefined)
  const opens = stubOpen(context, async () => ({ state: 'ready', ...readyMeta }))
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result } = renderHook(() => useRunLogDocument('github__1__o/r/w', 7, '99', true), { wrapper })

  await waitFor(() => assert.equal(result.current.data?.state, 'ready'))
  await act(async () => advance(300_000))

  assert.equal(opens.mock.callCount(), 1)
})

type Handler = (payload: {
  pipelineId: string
  runNumber: number
  nodeId?: string
  receivedBytes?: number
  revision?: number
}) => void

function captureListeners(context: TestContext, defer?: Promise<void>) {
  const handlers = new Map<string, Handler>()
  const unlisten = context.mock.fn(() => undefined)
  context.mock.method(events, 'listen', async (name: string, handler: Handler) => {
    handlers.set(name, handler)
    await defer

    return unlisten
  })

  return { handlers, unlisten }
}

function stubOpen(context: TestContext, impl: () => Promise<RunLogDocument>) {
  const opens = context.mock.fn(impl)
  context.mock.method(apiService, 'openRunLog', opens)
  context.mock.method(tauriService, 'openRunLog', opens)

  return opens
}

const target = { pipelineId: 'github__1__o/r/w', runNumber: 7, nodeId: '99' }

test('a failed open exposes the error', async (context) => {
  captureListeners(context)
  const opens = stubOpen(context, async () => {
    throw new Error('log unavailable')
  })
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result } = renderHook(() => useRunLogDocument(target.pipelineId, 7, '99', true), { wrapper })

  await waitFor(() => assert.equal(opens.mock.callCount(), 1))
  await act(async () => advance(1000))
  await waitFor(() => assert.equal(result.current.isError, true))
})

test('progress events update a loading document and an empty ready document is accepted', async (context) => {
  const { handlers } = captureListeners(context)
  let current: RunLogDocument = { state: 'loading', received_bytes: 1 }
  stubOpen(context, async () => current)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result } = renderHook(() => useRunLogDocument(target.pipelineId, 7, '99', true), { wrapper })
  await waitFor(() => assert.equal(result.current.data?.state, 'loading'))

  await act(async () => handlers.get('run-log-progress')?.({ ...target, receivedBytes: 4096 }))
  await waitFor(() => assert.deepEqual(result.current.data, { state: 'loading', received_bytes: 4096 }))

  current = { state: 'ready', ...readyMeta, line_count: 0, text_bytes: 0 }
  await act(async () => handlers.get('run-log-ready')?.(target))
  await waitFor(() => assert.equal(result.current.data?.state, 'ready'))
})

test('events for another target are ignored after switching targets', async (context) => {
  const { handlers } = captureListeners(context)
  const opens = stubOpen(context, async () => ({ state: 'loading', received_bytes: 1 }))
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result, rerender } = renderHook(({ node }) => useRunLogDocument(target.pipelineId, 7, node, true), {
    wrapper,
    initialProps: { node: '99' },
  })
  await waitFor(() => assert.equal(result.current.data?.state, 'loading'))
  rerender({ node: '100' })
  await waitFor(() => assert.equal(opens.mock.callCount(), 2))
  await waitFor(() => assert.equal(result.current.data?.state, 'loading'))

  await act(async () => handlers.get('run-log-progress')?.({ ...target, receivedBytes: 9999 }))

  assert.deepEqual(result.current.data, { state: 'loading', received_bytes: 1 })
})

test('listeners that register after unmount are released', async (context) => {
  const gate = Promise.withResolvers<void>()
  const { unlisten } = captureListeners(context, gate.promise)
  stubOpen(context, async () => ({ state: 'pending' }))
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { unmount } = renderHook(() => useRunLogDocument(target.pipelineId, 7, '99', true), { wrapper })
  unmount()
  gate.resolve()

  await waitFor(() => assert.equal(unlisten.mock.callCount(), 4))
})

test('a pending job opens when its run tree changes and a live one follows its own events', async (context) => {
  const { handlers } = captureListeners(context)
  let current: RunLogDocument = { state: 'pending' }
  const opens = stubOpen(context, async () => current)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const { result } = renderHook(() => useRunLogDocument(target.pipelineId, 7, '99', true), { wrapper })
  await waitFor(() => assert.equal(result.current.data?.state, 'pending'))

  await act(async () => handlers.get('run-log-tree-changed')?.({ pipelineId: 'other', runNumber: 7 }))
  assert.equal(opens.mock.callCount(), 1)

  current = { state: 'ready', ...readyMeta, complete: false }
  await act(async () => handlers.get('run-log-tree-changed')?.({ pipelineId: target.pipelineId, runNumber: 7 }))
  await waitFor(() => assert.equal(result.current.data?.state, 'ready'))
  assert.equal(opens.mock.callCount(), 2)

  await act(async () => handlers.get('run-log-tree-changed')?.({ pipelineId: target.pipelineId, runNumber: 7 }))
  assert.equal(opens.mock.callCount(), 2)

  current = { state: 'ready', ...readyMeta, complete: false, revision: 4, line_count: 20 }
  await act(async () => handlers.get('run-log-ready')?.({ ...target, revision: 4 }))
  await waitFor(() => assert.equal(result.current.data?.state === 'ready' && result.current.data.revision, 4))
})

function stubTree(context: TestContext, impl: () => Promise<RunLogTree>) {
  const trees = context.mock.fn(impl)
  context.mock.method(apiService, 'getRunLogTree', trees)
  context.mock.method(tauriService, 'getRunLogTree', trees)

  return trees
}

const tree = (complete: boolean): RunLogTree => ({ nodes: [], default_node_id: null, complete })

test('a run-log-tree-changed event refetches only the tree of that run', async (context) => {
  const { handlers } = captureListeners(context)
  const trees = stubTree(context, async () => tree(false))
  notifyManager.setNotifyFunction((callback) => act(callback))

  renderHook(() => useRunLogTree(target.pipelineId, 7, true), { wrapper })
  await waitFor(() => assert.equal(trees.mock.callCount(), 1))

  await act(async () => handlers.get('run-log-tree-changed')?.({ pipelineId: target.pipelineId, runNumber: 8 }))
  assert.equal(trees.mock.callCount(), 1)

  await act(async () => handlers.get('run-log-tree-changed')?.({ pipelineId: target.pipelineId, runNumber: 7 }))
  await waitFor(() => assert.equal(trees.mock.callCount(), 2))
})

test('an incomplete tree beats every 30 s and a complete one never does', async (context) => {
  context.mock.method(events, 'listen', async () => () => undefined)
  let complete = false
  const trees = stubTree(context, async () => tree(complete))
  notifyManager.setNotifyFunction((callback) => act(callback))

  const live = renderHook(() => useRunLogTree(target.pipelineId, 7, true), { wrapper })
  await waitFor(() => assert.equal(trees.mock.callCount(), 1))

  await act(async () => advance(29_999))
  assert.equal(trees.mock.callCount(), 1)
  complete = true
  await act(async () => advance(1))
  await waitFor(() => assert.equal(live.result.current.data?.complete, true))
  assert.equal(trees.mock.callCount(), 2)

  await act(async () => advance(300_000))
  assert.equal(trees.mock.callCount(), 2)
})
