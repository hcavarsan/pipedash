import assert from 'node:assert/strict'
import { test } from 'node:test'

import { getErrorCode, isPipedashError, isTransientError, toPipedashError } from './errors'

test('getErrorCode reads web errors, command rejections and wrapped causes', () => {
  assert.equal(getErrorCode(Object.assign(new Error('x'), { code: 'NOT_FOUND' })), 'NOT_FOUND')
  assert.equal(getErrorCode({ error: 'gone', code: 'NOT_FOUND' }), 'NOT_FOUND')
  assert.equal(getErrorCode(toPipedashError({ error: 'bad', code: 'INVALID_SEARCH' })), 'INVALID_SEARCH')
})

test('getErrorCode is undefined without a string code', () => {
  assert.equal(getErrorCode(new Error('x')), undefined)
  assert.equal(getErrorCode({ code: 404 }), undefined)
  assert.equal(getErrorCode('NOT_FOUND'), undefined)
  assert.equal(getErrorCode(null), undefined)
})

test('isPipedashError rejects unknown discriminants and variants missing required fields', () => {
  assert.equal(isPipedashError({ type: 'bogus', message: 'x' }), false)
  assert.equal(isPipedashError({ type: 'validation', message: 'x' }), false)
  assert.equal(isPipedashError({ type: 'permission', message: 'x', missingPermissions: 'repo' }), false)
  assert.equal(isPipedashError({ type: 'network', message: 'x', statusCode: '500' }), false)
  assert.equal(isPipedashError({ type: 'validation', message: 'x', fields: { a: 'b' } }), true)
  assert.equal(isPipedashError({ type: 'timeout', message: 'x' }), true)
})

test('toPipedashError normalizes malformed typed objects to unknown errors', () => {
  assert.equal(toPipedashError({ type: 'validation', message: 'x' }).type, 'unknown')
})

const mapped: [string, number, string][] = [
  ['UNAUTHORIZED', 401, 'auth'],
  ['PROVIDER_AUTH_FAILED', 401, 'auth'],
  ['FORBIDDEN', 403, 'permission'],
  ['NOT_FOUND', 404, 'not_found'],
  ['TIMEOUT', 504, 'timeout'],
  ['NETWORK_ERROR', 502, 'network'],
  ['SERVICE_UNAVAILABLE', 503, 'network'],
  ['SERVER_NOT_READY', 503, 'network'],
  ['UNREACHABLE', 502, 'network'],
]

for (const [code, status, type] of mapped) {
  test(`${code} classifies as ${type} for web errors and desktop envelopes`, () => {
    assert.equal(toPipedashError(Object.assign(new Error('x'), { status, code })).type, type)
    assert.equal(toPipedashError({ error: 'x', code }).type, type)
  })
}

test('an unknown PipedashError is reclassified from the code on its cause', () => {
  const cause = { error: 'down', code: 'SERVICE_UNAVAILABLE' }
  const error = toPipedashError({ type: 'unknown', message: 'down', cause })

  assert.equal(error.type, 'network')
  assert.equal(error.message, 'down')
  assert.equal(error.cause !== undefined, true)
})

test('isTransientError accepts network and timeout failures only', () => {
  const web = (status: number, code?: string) => Object.assign(new Error('x'), { status, code })

  for (const code of ['NETWORK_ERROR', 'TIMEOUT', 'SERVICE_UNAVAILABLE', 'SERVER_NOT_READY', 'UNREACHABLE']) {
    assert.equal(isTransientError({ error: 'x', code }), true, code)
  }
  for (const status of [502, 503, 504]) {
    assert.equal(isTransientError(web(status)), true, String(status))
  }
  assert.equal(isTransientError(web(429, 'RATE_LIMITED')), false)
  assert.equal(isTransientError(web(404, 'NOT_FOUND')), false)
  assert.equal(isTransientError(web(401, 'UNAUTHORIZED')), false)
  assert.equal(isTransientError(web(503, 'HOST_LOCKED')), false)
  assert.equal(isTransientError(web(502, 'TLS_ERROR')), false)
  assert.equal(isTransientError(web(501)), false)
})
