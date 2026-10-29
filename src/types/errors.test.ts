import assert from 'node:assert/strict'
import { test } from 'node:test'

import { getErrorCode, isPipedashError, toPipedashError } from './errors'

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
