import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { PaginatedRunHistory, Pipeline, PipelineRun } from '../types'

import {
  buildCancelGroups,
  cancellableGroups,
  countRuns,
  findUnfinishedRuns,
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

const pageOf = (runs: PipelineRun[], hasMore: boolean): PaginatedRunHistory => ({
  runs,
  total_count: runs.length,
  has_more: hasMore,
  is_complete: !hasMore,
  total_pages: 1,
  page: 1,
  page_size: 20,
})

const pagesFor =
  (pages: Record<string, PaginatedRunHistory[]>, calls: string[] = []) =>
  async (pipelineId: string, page: number) => {
    calls.push(`${pipelineId}:${page}`)

    return pages[pipelineId][page - 1]
  }

test('finds an older running run when the newest run finished', async () => {
  const fetchPage = pagesFor({ a: [pageOf([run(3, { status: 'success' }), run(2, { status: 'running' })], false)] })

  const { runs, failed } = await findUnfinishedRuns([pipeline('a', { status: 'success' })], fetchPage)

  assert.deepEqual(
    runs.a.map((item) => item.run_number),
    [2]
  )
  assert.deepEqual(failed, [])
})

test('follows the next page while the last one had unfinished runs and has_more', async () => {
  const calls: string[] = []
  const fetchPage = pagesFor(
    {
      a: [
        pageOf([run(5), run(4, { status: 'success' })], true),
        pageOf([run(3, { status: 'pending' }), run(2, { status: 'success' })], false),
      ],
    },
    calls
  )

  const { runs } = await findUnfinishedRuns([pipeline('a')], fetchPage)

  assert.deepEqual(calls, ['a:1', 'a:2'])
  assert.deepEqual(
    runs.a.map((item) => item.run_number),
    [5, 3]
  )
})

test('stops after a page without unfinished runs even when more pages exist', async () => {
  const calls: string[] = []
  const fetchPage = pagesFor(
    {
      a: [
        pageOf([run(5), run(4, { status: 'success' })], true),
        pageOf([run(3, { status: 'success' })], true),
        pageOf([run(2, { status: 'running' })], false),
      ],
    },
    calls
  )

  const { runs } = await findUnfinishedRuns([pipeline('a')], fetchPage)

  assert.deepEqual(calls, ['a:1', 'a:2'])
  assert.deepEqual(
    runs.a.map((item) => item.run_number),
    [5]
  )
})

test('pipelines without unfinished runs are omitted and failed fetches are recorded', async () => {
  const progress: number[] = []
  const ok = pageOf([run(1, { status: 'success' })], false)
  const fetchPage = async (pipelineId: string, page: number) => {
    if (pipelineId === 'broken') {
      throw new Error('boom')
    }

    return pagesFor({ idle: [ok] })(pipelineId, page)
  }

  const result = await findUnfinishedRuns([pipeline('idle'), pipeline('broken')], fetchPage, 4, (done) =>
    progress.push(done)
  )

  assert.deepEqual(result.runs, {})
  assert.deepEqual(
    result.failed.map((item) => item.id),
    ['broken']
  )
  assert.deepEqual(progress, [1, 2])
})

test('runs found before a later page fails are kept and the pipeline is reported', async () => {
  const fetchPage = async (_pipelineId: string, page: number) => {
    if (page === 2) {
      throw new Error('boom')
    }

    return pageOf([run(5, { status: 'running' })], true)
  }

  const result = await findUnfinishedRuns([pipeline('a')], fetchPage)

  assert.deepEqual(
    result.runs.a.map((item) => item.run_number),
    [5]
  )
  assert.deepEqual(
    result.failed.map((item) => item.id),
    ['a']
  )
})

test('never has more requests in flight than the concurrency limit', async () => {
  let inFlight = 0
  let peak = 0
  const fetchPage = async () => {
    inFlight++
    peak = Math.max(peak, inFlight)
    await Promise.resolve()
    inFlight--

    return pageOf([run(1)], false)
  }
  const pipelines = Array.from({ length: 10 }, (_, index) => pipeline(`p${index}`))

  const { runs } = await findUnfinishedRuns(pipelines, fetchPage, 3)

  assert.equal(peak, 3)
  assert.equal(Object.keys(runs).length, 10)
})
