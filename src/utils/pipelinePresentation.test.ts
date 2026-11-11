import assert from 'node:assert/strict'
import { test } from 'node:test'

import { getPipelineStatusLabel, getRunStatusLabel, pipelineStatusKey } from './pipelinePresentation'

test('pipeline label prefers overdue over the resource state label', () => {
  assert.equal(
    getPipelineStatusLabel({ overdue_run: 'run-1', resource_state: { label: 'Synced' } }),
    'Running longer than usual'
  )
  assert.equal(getPipelineStatusLabel({ resource_state: { label: 'Synced' } }), 'Synced')
  assert.equal(getPipelineStatusLabel({}), undefined)
})

test('run label comes from run metadata and marks archived runs as cached', () => {
  assert.equal(getRunStatusLabel({ metadata: { status_label: 'Reconciling' } }), 'Reconciling')
  assert.equal(getRunStatusLabel({ metadata: { status_label: 'Ready', archived: true } }), 'Ready (cached)')
  assert.equal(getRunStatusLabel({}), undefined)
})

test('only a pending pipeline with no run and no resource state has never run', () => {
  assert.equal(pipelineStatusKey({ status: 'pending', last_run: null }), 'never')
  assert.equal(pipelineStatusKey({ status: 'pending', last_run: '2026-01-01T00:00:00Z' }), 'pending')
  assert.equal(
    pipelineStatusKey({ status: 'pending', last_run: null, resource_state: { label: 'Waiting' } }),
    'pending'
  )
  assert.equal(pipelineStatusKey({ status: 'failed', last_run: null }), 'failed')
})
