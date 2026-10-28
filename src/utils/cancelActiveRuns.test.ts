import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { Pipeline, PipelineRun } from '../types'

import {
  buildCancelGroups,
  cancellableGroups,
  countRuns,
  isUnfinished,
  summarizeCancellation,
  unfinishedPipelines,
} from './cancelActiveRuns'

const pipeline = (id: string, overrides: Partial<Pipeline> = {}): Pipeline => ({
  id,
  provider_id: 1,
  provider_type: 'github',
  name: id,
  status: 'running',
  last_run: null,
  last_updated: '2026-10-28T10:00:00Z',
  repository: 'acme/storefront',
  branch: 'main',
  workflow_file: null,
  ...overrides,
})

const run = (number: number, overrides: Partial<PipelineRun> = {}): PipelineRun => ({
  id: `r${number}`,
  pipeline_id: 'p',
  run_number: number,
  status: 'running',
  started_at: '2026-10-28T10:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: 'main',
  actor: null,
  ...overrides,
})

const provider = (item: Pipeline) => ({ name: item.provider_type, canCancel: item.provider_type !== 'readonly' })

test('only running and pending statuses are unfinished', () => {
  assert.deepEqual((['running', 'pending', 'success', 'failed', 'cancelled', 'skipped'] as const).map(isUnfinished), [
    true,
    true,
    false,
    false,
    false,
    false,
  ])
})

test('unfinished pipelines drop finished ones', () => {
  const items = [pipeline('a'), pipeline('b', { status: 'success' }), pipeline('c', { status: 'pending' })]

  assert.deepEqual(
    unfinishedPipelines(items).map((item) => item.id),
    ['a', 'c']
  )
})

test('groups keep only unfinished runs and skip pipelines with none', () => {
  const items = [pipeline('a'), pipeline('b'), pipeline('c')]
  const groups = buildCancelGroups(
    items,
    { a: [run(2), run(1, { status: 'success' }), run(3, { status: 'pending' })], b: [run(9, { status: 'failed' })] },
    provider
  )

  assert.deepEqual(
    groups.map((group) => [group.pipeline.id, group.runs.map((item) => item.run_number)]),
    [['a', [2, 3]]]
  )
  assert.equal(countRuns(groups), 2)
})

test('providers that cannot cancel are flagged and excluded from the cancellable set', () => {
  const items = [pipeline('a'), pipeline('b', { provider_type: 'readonly' })]
  const groups = buildCancelGroups(items, { a: [run(1)], b: [run(2), run(3)] }, provider)

  assert.deepEqual(
    groups.map((group) => group.canCancel),
    [true, false]
  )
  assert.deepEqual(
    cancellableGroups(groups).map((group) => group.pipeline.id),
    ['a']
  )
  assert.equal(countRuns(cancellableGroups(groups)), 1)
})

test('summary is quiet when every run was cancelled', () => {
  const summary = summarizeCancellation([
    { pipelineId: 'a', pipelineName: 'CI', runNumber: 1, error: null },
    { pipelineId: 'a', pipelineName: 'CI', runNumber: 2, error: null },
  ])

  assert.equal(summary.message, 'Cancellation requested for 2 runs')
  assert.equal(summary.color, 'gray')
})

test('summary names each failed run with its reason', () => {
  const summary = summarizeCancellation([
    { pipelineId: 'a', pipelineName: 'CI', runNumber: 1, error: null },
    { pipelineId: 'a', pipelineName: 'CI', runNumber: 2, error: 'Forbidden' },
    { pipelineId: 'b', pipelineName: 'Deploy', runNumber: 7, error: 'Already finished' },
  ])

  assert.equal(
    summary.message,
    'Cancellation requested for 1 of 3 runs. Failed: CI #2 (Forbidden); Deploy #7 (Already finished)'
  )
  assert.equal(summary.color, 'red')
})
