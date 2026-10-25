import assert from 'node:assert/strict'
import { test } from 'node:test'

import { createError, formatErrorMessage, toPipedashError } from '../types/errors'
import { withRetry } from './retryLogic'

const noRetry = { maxAttempts: 1, delayMs: 0 }

test('a Tauri command error keeps the backend message', () => {
  const commandError = { error: 'Failed to create GitHub client: no CA certificates found', details: null }

  assert.equal(formatErrorMessage(commandError), commandError.error)
  assert.equal(toPipedashError(commandError).message, commandError.error)
})

test('withRetry rethrows the original error instead of stringifying it', async () => {
  const original = createError('auth', 'Bad credentials')

  await assert.rejects(
    withRetry(() => Promise.reject(original), noRetry),
    (error: unknown) => {
      assert.equal(error, original)
      assert.equal(formatErrorMessage(error), 'Bad credentials')

      return true
    }
  )
})

test('withRetry still retries errors that shouldRetry accepts', async () => {
  let calls = 0

  const result = await withRetry(
    async () => {
      calls++
      if (calls < 3) {
        throw createError('network', 'NetworkError when attempting to fetch')
      }

      return 'ok'
    },
    { maxAttempts: 3, delayMs: 0, shouldRetry: (error) => error.message.includes('NetworkError') }
  )

  assert.equal(result, 'ok')
  assert.equal(calls, 3)
})
