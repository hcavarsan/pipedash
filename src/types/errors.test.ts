import assert from 'node:assert/strict'
import { test } from 'node:test'

import { getErrorCode, toPipedashError } from './errors'

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
