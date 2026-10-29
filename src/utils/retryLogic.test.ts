import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatErrorMessage, type PipedashError, toPipedashError } from '../types/errors'
import { withRetry } from './retryLogic'

function authError(): PipedashError {
  return { type: 'auth', message: 'Bad credentials' }
}

function networkError(): PipedashError {
  return { type: 'network', message: 'NetworkError when attempting to fetch' }
}

const noRetry = { maxAttempts: 1, delayMs: 0 }

test('a Tauri command error keeps the backend message', () => {
  const commandError = { error: 'Failed to create GitHub client: no CA certificates found', details: null }

  assert.equal(formatErrorMessage(commandError), commandError.error)
  assert.equal(toPipedashError(commandError).message, commandError.error)
})

test('withRetry rethrows the original error instead of stringifying it', async () => {
  const original = authError()

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
        throw networkError()
      }

      return 'ok'
    },
    { maxAttempts: 3, delayMs: 0, shouldRetry: (error) => error.message.includes('NetworkError') }
  )

  assert.equal(result, 'ok')
  assert.equal(calls, 3)
})

test('retries until the attempt limit and rejects with the final error', async () => {
  let calls = 0
  const final = new Error('NetworkError last')

  await assert.rejects(
    withRetry(
      async () => {
        calls += 1
        throw calls === 3 ? final : new Error('NetworkError again')
      },
      { maxAttempts: 3, delayMs: 0 }
    ),
    final
  )
  assert.equal(calls, 3)
})
