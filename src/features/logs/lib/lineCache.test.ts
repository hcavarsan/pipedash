import assert from 'node:assert/strict'
import { after, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { LogLinesPage } from '../../../types/logs'
import type { DocRef, LineState } from './lineCache'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { apiService } = await import('../../../services/api')
const { LogLineCache } = await import('./lineCache')

after(async () => {
  await GlobalRegistrator.unregister()
})

interface Call {
  start: number
  count: number
  release: () => void
  fail: (error: unknown) => void
}

const doc = (lineCount = 3_000_000, overrides: Partial<DocRef> = {}): DocRef => ({
  pipelineId: 'github__1__o/r/w',
  runNumber: 7,
  nodeId: '99',
  revision: 1,
  lineCount,
  previousRevision: null,
  stableLines: 0,
  ...overrides,
})

const flush = () => {
  const { promise, resolve } = Promise.withResolvers<void>()

  setImmediate(resolve)

  return promise
}

async function waitFor(check: () => void): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    try {
      check()

      return
    } catch {
      await flush()
    }
  }

  check()
}
const textOf = (state: LineState) => (typeof state === 'object' ? state.text : state)

const pageOf = (start: number, count: number, text: (n: number) => string): LogLinesPage => ({
  start,
  lines: Array.from({ length: count }, (_, i) => ({ text: text(start + i) })),
})

function gatedReader(
  context: TestContext,
  text: (n: number) => string = (n) => `line ${n}`,
  emptyCalls: ReadonlySet<number> = new Set()
) {
  const calls: Call[] = []
  let inFlight = 0
  let peak = 0

  context.mock.method(
    apiService,
    'readRunLogLines',
    async (_p: string, _r: number, _n: string, _rev: number, start: number, count: number) => {
      const gate = Promise.withResolvers<void>()
      const index = calls.length

      calls.push({ start, count, release: gate.resolve, fail: gate.reject })
      inFlight += 1
      peak = Math.max(peak, inFlight)
      try {
        await gate.promise
      } finally {
        inFlight -= 1
      }

      return emptyCalls.has(index) ? { start, lines: [] } : pageOf(start, count, text)
    }
  )

  return { calls, peak: () => peak }
}

test('rows at both ends of a 3M-line log request only the lines around them', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [1, 2_999_999])
  await waitFor(() => assert.equal(reader.calls.length, 2))

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
  await waitFor(() => assert.equal(textOf(cache.line(doc(), 2_999_999)), 'line 2999999'))
  assert.deepEqual(cache.line(doc(), 1), { text: 'line 1', kind: 'plain', ts: null, spans: [], omitted: 0 })
})

test('at most two requests are in flight and the rest wait their turn', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [0, 1000, 2000, 3000, 4000])
  await waitFor(() => assert.equal(reader.calls.length, 2))

  for (let i = 0; i < 5; i += 1) {
    const call = reader.calls[i]

    assert.ok(call)
    call.release()
    await waitFor(() => assert.equal(reader.calls.length, Math.min(i + 3, 5)))
  }

  await waitFor(() => assert.equal(textOf(cache.line(doc(), 4000)), 'line 4000'))
  assert.equal(reader.peak(), 2)
})

test('queued requests that nobody needs any more are dropped', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache(undefined, 1)
  const owner = Symbol('a')

  cache.need(owner, doc(), [0, 1000, 2000])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  cache.need(owner, doc(), [2000])
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(reader.calls.length, 2))

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
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(typeof cache.line(doc(100), 10), 'object'))

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
  const owner = Symbol('a')

  cache.need(owner, doc(), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  cache.release(owner)
  cache.clear()
  reader.calls[0]?.release()
  await flush()

  assert.equal(cache.line(doc(), 0), undefined)
})

test('clear() reloads the lines an active owner still needs', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(typeof cache.line(doc(), 0), 'object'))

  cache.clear()
  await waitFor(() => assert.equal(reader.calls.length, 2))
  assert.equal(cache.line(doc(), 0), 'loading')

  reader.calls[1]?.release()
  await waitFor(() => assert.equal(textOf(cache.line(doc(), 0)), 'line 0'))
})

test('a stale response after clear() does not fill the lines being reloaded', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  cache.clear()
  await waitFor(() => assert.equal(reader.calls.length, 2))
  reader.calls[0]?.release()
  await flush()

  assert.equal(cache.line(doc(), 0), 'loading')
})

test('an empty page before the range is read leaves the lines retryable', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context, undefined, new Set([0]))
  const cache = new LogLineCache()

  cache.need(Symbol('a'), doc(), [5])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(cache.line(doc(), 5), 'error'))

  context.mock.timers.tick(5000)
  assert.equal(reader.calls.length, 2)
  reader.calls[1]?.release()
  await waitFor(() => assert.equal(textOf(cache.line(doc(), 5)), 'line 5'))
  cache.clear()
})

test('reading a line does not change which lines are evicted first', async (context) => {
  const reader = gatedReader(context, () => 'x'.repeat(1000))
  const cache = new LogLineCache(4500)
  const first = Symbol('a')

  cache.need(first, doc(3), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(typeof cache.line(doc(3), 2), 'object'))
  cache.release(first)

  cache.line(doc(3), 0)
  cache.need(Symbol('b'), doc(3), [1])
  await waitFor(() => assert.equal(reader.calls.length, 2))
  reader.calls[1]?.release()
  await waitFor(() => assert.equal(typeof cache.line(doc(3), 1), 'object'))

  assert.equal(cache.line(doc(3), 0), undefined)
  assert.equal(typeof cache.line(doc(3), 2), 'object')
})

test('a failed request is retried after 5 s while still needed, and not once released', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const owner = Symbol('a')

  cache.need(owner, doc(), [5])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.fail(new Error('boom'))
  await waitFor(() => assert.equal(cache.line(doc(), 5), 'error'))

  context.mock.timers.tick(4999)
  assert.equal(reader.calls.length, 1)
  context.mock.timers.tick(1)
  assert.equal(reader.calls.length, 2)
  assert.equal(cache.line(doc(), 5), 'loading')

  reader.calls[1]?.fail(new Error('boom'))
  await waitFor(() => assert.equal(cache.line(doc(), 5), 'error'))
  cache.release(owner)
  context.mock.timers.tick(5000)
  await flush()
  assert.equal(reader.calls.length, 2)
})

test('a NOT_FOUND error notifies onRevisionGone and other codes do not', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const gone: DocRef[] = []

  cache.onRevisionGone((ref) => gone.push(ref))
  cache.need(Symbol('a'), doc(), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.fail(Object.assign(new Error('This log revision is no longer available'), { code: 'INTERNAL' }))
  await waitFor(() => assert.equal(cache.line(doc(), 0), 'error'))
  assert.deepEqual(gone, [])
  context.mock.timers.tick(5000)
  assert.equal(reader.calls.length, 2)
  reader.calls[1]?.fail(Object.assign(new Error('x'), { code: 'NOT_FOUND' }))
  await waitFor(() => assert.deepEqual(gone, [doc()]))
  cache.clear()
})

test('a command error shaped as { error, code } also counts', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const gone: DocRef[] = []

  cache.onRevisionGone((ref) => gone.push(ref))
  cache.need(Symbol('a'), doc(), [0])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.fail({ error: 'gone', code: 'NOT_FOUND' })
  await waitFor(() => assert.deepEqual(gone, [doc()]))
  cache.clear()
})

test('a continuing revision reuses its stable lines and fetches only the new ones', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const owner = Symbol('a')
  const first = doc(100)

  cache.need(owner, first, [0, 1, 2])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(textOf(cache.line(first, 2)), 'line 2'))

  const second = doc(150, { revision: 2, previousRevision: 1, stableLines: 100 })

  cache.need(owner, second, [0, 1, 2, 120])
  assert.equal(textOf(cache.line(second, 2)), 'line 2')
  await waitFor(() => assert.equal(reader.calls.length, 2))
  await flush()

  assert.deepEqual(
    reader.calls.map(({ start, count }) => [start, count]),
    [
      [0, 64],
      [64, 64],
    ]
  )
})

test('lines at or after stable_lines and non-consecutive revisions are fetched again', async (context) => {
  const reader = gatedReader(context)
  const cache = new LogLineCache()
  const owner = Symbol('a')
  const first = doc(100)

  cache.need(owner, first, [0, 1, 2])
  await waitFor(() => assert.equal(reader.calls.length, 1))
  reader.calls[0]?.release()
  await waitFor(() => assert.equal(textOf(cache.line(first, 2)), 'line 2'))

  const second = doc(100, { revision: 2, previousRevision: 1, stableLines: 2 })

  cache.need(owner, second, [0, 2])
  assert.equal(textOf(cache.line(second, 1)), 'line 1')
  assert.equal(cache.line(second, 2), 'loading')
  await waitFor(() => assert.equal(reader.calls.length, 2))
  assert.deepEqual([reader.calls[1]?.start, reader.calls[1]?.count], [2, 62])
  reader.calls[1]?.release()
  await waitFor(() => assert.equal(textOf(cache.line(second, 2)), 'line 2'))

  const skipped = doc(100, { revision: 4, previousRevision: 3, stableLines: 100 })

  cache.need(owner, skipped, [0])
  assert.equal(cache.line(skipped, 0), 'loading')
})
