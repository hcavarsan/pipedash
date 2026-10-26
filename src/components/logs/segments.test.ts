import assert from 'node:assert/strict'
import { test } from 'node:test'

import { segmentsFor } from './segments'

test('plain text is a single unstyled segment and empty text has none', () => {
  assert.deepEqual(segmentsFor('hello', [], []), [{ text: 'hello', style: null, match: 0 }])
  assert.deepEqual(segmentsFor('', [[0, 3, 1]], []), [])
})

test('spans and highlights split at every edge', () => {
  const result = segmentsFor('abcdefghij', [[2, 6, 3]], [[4, 8, false]])

  assert.deepEqual(
    result.map(({ text, style, match }) => [text, style, match]),
    [
      ['ab', null, 0],
      ['cd', 3, 0],
      ['ef', 3, 1],
      ['gh', null, 1],
      ['ij', null, 0],
    ]
  )
})

test('the current match is flagged 2 and wins over a plain match', () => {
  const result = segmentsFor(
    'foo foo',
    [],
    [
      [0, 3, false],
      [0, 3, true],
      [4, 7, false],
    ]
  )

  assert.deepEqual(
    result.map(({ text, match }) => [text, match]),
    [
      ['foo', 2],
      [' ', 0],
      ['foo', 1],
    ]
  )
})

test('spans past the text are clipped', () => {
  assert.deepEqual(segmentsFor('abc', [[1, 99, 0]], []), [
    { text: 'a', style: null, match: 0 },
    { text: 'bc', style: 0, match: 0 },
  ])
})

test('urls become links without trailing punctuation', () => {
  const result = segmentsFor('see https://example.com/a?b=1, now (http://x.io/y).', [], [])

  assert.deepEqual(
    result.filter((segment) => segment.href).map((segment) => segment.href),
    ['https://example.com/a?b=1', 'http://x.io/y']
  )
  assert.equal(result.map((segment) => segment.text).join(''), 'see https://example.com/a?b=1, now (http://x.io/y).')
})

test('a highlight inside a url keeps the link on both pieces', () => {
  const result = segmentsFor('https://example.com/path', [], [[8, 19, true]])

  assert.deepEqual(
    result.map(({ text, match, href }) => [text, match, href]),
    [
      ['https://', 0, 'https://example.com/path'],
      ['example.com', 2, 'https://example.com/path'],
      ['/path', 0, 'https://example.com/path'],
    ]
  )
})

test('a bare scheme is not a link', () => {
  assert.equal(
    segmentsFor('see https:// here', [], []).some((segment) => segment.href),
    false
  )
})
