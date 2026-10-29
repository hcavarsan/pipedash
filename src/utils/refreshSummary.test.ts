import assert from 'node:assert/strict'
import { test } from 'node:test'

import { providerSummaryFixture } from '../testing/fixtures'
import type { RefreshResult } from '../types'

import { describeRefreshResult, summarizeRefresh } from './refreshSummary'

const result = (provider_id: number, overrides: Partial<RefreshResult> = {}): RefreshResult => ({
  provider_id,
  state: 'ok',
  kind: null,
  message: null,
  ...overrides,
})

const provider = (id: number, name: string) => providerSummaryFixture({ id, name })

test('summarizes a clean refresh with the number of providers', () => {
  assert.deepEqual(summarizeRefresh([result(1)], []), { message: 'Refreshed 1 provider', color: 'green' })
  assert.deepEqual(summarizeRefresh([result(1), result(2, { state: 'recent' }), result(3), result(4)], []), {
    message: 'Refreshed 4 providers',
    color: 'green',
  })
})

test('names failures and counts providers that are still running', () => {
  const summary = summarizeRefresh(
    [
      result(1),
      result(2),
      result(3),
      result(4, { state: 'failed', kind: 'rate_limited' }),
      result(5, { state: 'pending' }),
    ],
    [provider(4, 'GitLab')]
  )

  assert.deepEqual(summary, {
    message: 'Refreshed 3 of 5 · GitLab: rate limited · 1 still running',
    color: 'orange',
  })
})

test('a skipped provider whose last check failed counts as failed, not refreshed', () => {
  const skipped = result(2, { state: 'recent', kind: 'rate_limited' })

  assert.deepEqual(summarizeRefresh([result(1), skipped], [provider(2, 'GitHub')]), {
    message: 'Refreshed 1 of 2 · GitHub: rate limited',
    color: 'orange',
  })
  assert.equal(describeRefreshResult(skipped), 'Not retried yet · last check rate limited')
})

test('a provider removed during the refresh is not counted as refreshed', () => {
  assert.deepEqual(summarizeRefresh([result(1, { state: 'not_found' })], []), {
    message: 'Refreshed 0 of 1 · 1 no longer exists',
    color: 'orange',
  })
  assert.deepEqual(
    summarizeRefresh([result(1), result(2, { state: 'not_found' }), result(3, { state: 'not_found' })], []),
    {
      message: 'Refreshed 1 of 3 · 2 no longer exist',
      color: 'orange',
    }
  )
})

test('describes a single result for the retry action', () => {
  assert.equal(describeRefreshResult(result(1, { state: 'recent' })), 'Checked a moment ago')
  assert.equal(describeRefreshResult(result(1, { state: 'failed', kind: 'auth', message: 'bad token' })), 'bad token')
  assert.equal(describeRefreshResult(result(1, { state: 'failed', kind: 'timeout' })), 'timed out')
})

test('an empty refresh reports zero providers refreshed', () => {
  assert.deepEqual(summarizeRefresh([], []), { message: 'Refreshed 0 providers', color: 'green' })
})

test('describes pending and removed providers for the retry action', () => {
  assert.equal(describeRefreshResult(result(1, { state: 'pending' })), 'Still checking')
  assert.equal(describeRefreshResult(result(1, { state: 'not_found' })), 'Provider no longer exists')
})
