import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { LogDocumentMeta, RunLogDocument } from '../types/logs'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider, timeoutManager } = await import(
  '@tanstack/react-query'
)
const { apiService, events, tauriService } = await import('../services')
const { useRunLogDocument } = await import('./useRunLogQueries')

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
}

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

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

test('a pending job becomes ready after the 10 s refetch without reselection', async (context) => {
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
  await act(async () => advance(9999))
  assert.equal(result.current.data?.state, 'pending')
  assert.equal(opens.mock.callCount(), 1)

  await act(async () => advance(1))
  await waitFor(() => assert.equal(result.current.data?.state, 'ready'))
  assert.equal(opens.mock.callCount(), 2)
})
