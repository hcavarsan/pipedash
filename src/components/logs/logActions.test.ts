import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { LogLinesPage } from '../../types/logs'

import {
  COPY_MAX_LINES,
  COPY_PAGE_LINES,
  CopyTooLargeError,
  collectLines,
  errorCounter,
  exportFileName,
  newSessionId,
} from './logActions'

const source =
  (lines: string[], calls: Array<[number, number]> = []) =>
  async (start: number, count: number): Promise<LogLinesPage> => {
    calls.push([start, count])

    return { start, lines: lines.slice(start, start + count).map((text) => ({ text })) }
  }

test('file names are slugged and capped at 255 bytes with the extension', () => {
  assert.equal(exportFileName('My Pipeline/main', 12, 'build & test'), 'My-Pipeline-main-run-12-build-test.log')

  const long = exportFileName('a'.repeat(400), 1, 'job')

  assert.equal(long.length, 255)
  assert.ok(long.endsWith('.log'))
})

test('the error counter shows the position of the selected error line, otherwise the total', () => {
  const lines = [3, 9, 20]

  assert.equal(errorCounter([], 0, null), 'No errors')
  assert.equal(errorCounter(lines, 3, null), '3 errors')
  assert.equal(errorCounter([4], 1, null), '1 error')
  assert.equal(errorCounter(lines, 3, 3), 'Error 1 of 3')
  assert.equal(errorCounter(lines, 3, 20), 'Error 3 of 3')
  assert.equal(errorCounter(lines, 3, 10), '3 errors')
  assert.equal(errorCounter(lines, 3, 99), '3 errors')
  // The server stores a capped number of error positions but still counts every error.
  assert.equal(errorCounter(lines, 900, 9), 'Error 2 of 900')
})

test('collectLines pages through the range and joins with newlines', async () => {
  const lines = Array.from({ length: COPY_PAGE_LINES + 5 }, (_, index) => `l${index}`)
  const calls: Array<[number, number]> = []
  const text = await collectLines(0, lines.length, source(lines, calls))

  assert.equal(text, lines.join('\n'))
  assert.deepEqual(calls, [
    [0, COPY_PAGE_LINES],
    [COPY_PAGE_LINES, 5],
  ])
})

test('too many lines fail before any request', async () => {
  const calls: Array<[number, number]> = []

  await assert.rejects(collectLines(0, COPY_MAX_LINES + 1, source([], calls)), CopyTooLargeError)
  assert.equal(calls.length, 0)
})

test('too many characters fail while paging', async () => {
  const big = 'x'.repeat(1_048_576)

  await assert.rejects(collectLines(0, 5, source([big, big, big, big, big])), CopyTooLargeError)
})

test('an empty page ends the loop', async () => {
  assert.equal(await collectLines(0, 10, source(['a', 'b'])), 'a\nb')
})

test('session ids are 32 hex characters and do not need randomUUID', () => {
  const original = crypto.randomUUID

  Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })

  try {
    const first = newSessionId()

    assert.match(first, /^[0-9a-f]{32}$/)
    assert.notEqual(first, newSessionId())
  } finally {
    Object.defineProperty(crypto, 'randomUUID', { value: original, configurable: true })
  }
})
