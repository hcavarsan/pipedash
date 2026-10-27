import assert from 'node:assert/strict'
import { after, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { LogLinesPage } from '../../types/logs'
import type { DocRef, LineState } from './lineCache'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { apiService } = await import('../../services')
const { LogLineCache } = await import('./lineCache')

after(() => {
  GlobalRegistrator.unregister()
})

interface Call {
  start: number
  count: number
  release: () => void
  fail: (error: unknown) => void
}

const doc = (lineCount = 3_000_000): DocRef => ({
  pipelineId: 'github__1__o/r/w',
  runNumber: 7,
  nodeId: '99',
  revision: 1,
  lineCount,
})

const settle = async () => {
  for (let i = 0; i < 30; i += 1) {
    await Promise.resolve()
  }
}

const textOf = (state: LineState) => (typeof state === 'object' ? state.text : state)

const pageOf = (start: number, count: number, text: (n: number) => string): LogLinesPage => ({
  start,
  lines: Array.from({ length: count }, (_, i) => ({ text: text(start + i) })),
})

function gatedReader(context: TestContext, text: (n: number) => string = (n) => `line ${n}`) {
  const calls: Call[] = []
  let inFlight = 0
  let peak = 0

  context.mock.method(
    apiService,
    'readRunLogLines',
    async (_p: string, _r: number, _n: string, _rev: number, start: number, count: number) => {
      const gate = Promise.withResolvers<void>()

      calls.push({ start, count, release: gate.resolve, fail: gate.reject })
      inFlight += 1
      peak = Math.max(peak, inFlight)
      try {
        await gate.promise
      } finally {
        inFlight -= 1
      }

      return pageOf(start, count, text)
    }
  )

  return { calls, peak: () => peak }
}

test('rows at both ends of a 3M-line log request only the lines around them', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [1, 2_999_999])
  await settle()

  assert.deepEqual(
    reader.calls.map(({ start, count }) => [start, count]),
    [
      [0, 64],
      [2_999_936, 64],
    ]
  )
  assert.equal(cache.line(doc(), 1), 'loading')
  for (const call of reader.calls) {
    call.release()
  }
  await settle()
  assert.deepEqual(cache.line(doc(), 1), { text: 'line 1', kind: 'plain', ts: null, spans: [], omitted: 0 })
  assert.equal(textOf(cache.line(doc(), 2_999_999)), 'line 2999999')
})

test('at most two requests are in flight and the rest wait their turn', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [0, 1000, 2000, 3000, 4000])
  await settle()
  assert.equal(reader.calls.length, 2)

  for (let i = 0; i < 5; i += 1) {
    reader.calls[i]?.release()
    await settle()
  }

  assert.equal(reader.calls.length, 5)
  assert.equal(reader.peak(), 2)
  assert.equal(textOf(cache.line(doc(), 4000)), 'line 4000')
})

test('queued requests that nobody needs any more are dropped', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache(undefined, 1)
  const owner = Symbol('a')

  cache.need(owner, doc(), [0, 1000, 2000])
  await settle()
  cache.need(owner, doc(), [2000])
  reader.calls[0]?.release()
  await settle()

  assert.deepEqual(
    reader.calls.map(({ start }) => start),
    [0, 1984]
  )
  assert.equal(cache.line(doc(), 1000), undefined)
})

test('the byte budget holds with long lines while needed lines stay', async (context) => {
  const reader = gatedReader(context, () => 'x'.repeat(1000))
  const cache = new LogLineCache(10_000)

  cache.need(Symbol('a'), doc(100), [10, 11, 12])
  await settle()
  reader.calls[0]?.release()
  await settle()

  const kept = Array.from({ length: 64 }, (_, n) => cache.line(doc(100), n)).filter(
    (state) => typeof state === 'object'
  )

  assert.ok(kept.length >= 3 && kept.length <= 4, `kept ${kept.length} lines`)
  for (const n of [10, 11, 12]) {
    assert.equal(typeof cache.line(doc(100), n), 'object')
  }
})

test('a response that lands after clear() is discarded', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [0])
  await settle()
  cache.clear()
  reader.calls[0]?.release()
  await settle()

  assert.equal(cache.line(doc(), 0), undefined)
})

test('a failed request is retried after 5 s while still needed, and not once released', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const owner = Symbol('a')

  cache.need(owner, doc(), [5])
  await settle()
  reader.calls[0]?.fail(new Error('boom'))
  await settle()
  assert.equal(cache.line(doc(), 5), 'error')

  context.mock.timers.tick(4999)
  await settle()
  assert.equal(reader.calls.length, 1)
  context.mock.timers.tick(1)
  await settle()
  assert.equal(reader.calls.length, 2)
  assert.equal(cache.line(doc(), 5), 'loading')

  reader.calls[1]?.fail(new Error('boom'))
  await settle()
  cache.release(owner)
  context.mock.timers.tick(5000)
  await settle()
  assert.equal(reader.calls.length, 2)
})

test('a NOT_FOUND error notifies onRevisionGone and other codes do not', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const gone: DocRef[] = []

  cache.onRevisionGone((ref) => gone.push(ref))
  cache.need(Symbol('a'), doc(), [0])
  await settle()
  reader.calls[0]?.fail(Object.assign(new Error('This log revision is no longer available'), { code: 'INTERNAL' }))
  await settle()
  assert.deepEqual(gone, [])
  context.mock.timers.tick(5000)
  await settle()
  reader.calls[1]?.fail(Object.assign(new Error('x'), { code: 'NOT_FOUND' }))
  await settle()

  assert.deepEqual(gone, [doc()])
  cache.clear()
})

test('a command error shaped as { error, code } also counts', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const gone: DocRef[] = []

  cache.onRevisionGone((ref) => gone.push(ref))
  cache.need(Symbol('a'), doc(), [0])
  await settle()
  reader.calls[0]?.fail({ error: 'gone', code: 'NOT_FOUND' })
  await settle()

  assert.deepEqual(gone, [doc()])
  cache.clear()
})
