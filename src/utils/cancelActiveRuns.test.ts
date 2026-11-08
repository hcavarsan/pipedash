import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { CancellationBatch } from '../types/run_action_batch'
import { summarizeCancellation } from './cancelActiveRuns'

const result = (run_number: number, error: string | null) => ({
  target: { pipeline_id: 'p', run_number },
  pipeline_name: 'build',
  error,
})

test('reports success when every run was cancelled', () => {
  const batch: CancellationBatch = { results: [result(1, null)], requested_count: 1, failed_count: 0 }

  assert.deepEqual(summarizeCancellation(batch), {
    title: 'Cancellation requested',
    message: 'Cancellation requested for 1 run',
    color: 'gray',
  })
})

test('lists failures from the backend batch', () => {
  const batch: CancellationBatch = {
    results: [result(1, null), result(2, 'denied')],
    requested_count: 2,
    failed_count: 1,
  }
  const note = summarizeCancellation(batch)

  assert.equal(note.color, 'red')
  assert.match(String(note.message), /1 of 2 runs\. Failed: build #2 \(denied\)/)
})
