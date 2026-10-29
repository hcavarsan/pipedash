import assert from 'node:assert/strict'
import { test } from 'node:test'

import { QueryClient } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'
import type { PaginatedRunHistory, PipelineRun } from '../../../types/domain'
import { updateRunInHistory } from './runHistoryCache'

const run = (runNumber: number, status: PipelineRun['status'], pipelineId = 'p1'): PipelineRun => ({
  id: `r${runNumber}`,
  pipeline_id: pipelineId,
  run_number: runNumber,
  status,
  started_at: '2026-01-01T00:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: null,
  actor: null,
})

const page = (...runs: PipelineRun[]): PaginatedRunHistory => ({
  runs,
  total_count: runs.length,
  has_more: false,
  is_complete: true,
  page: 1,
  page_size: 20,
  total_pages: 1,
})

test('a run update replaces that run in paged and infinite histories of its pipeline only', () => {
  const client = new QueryClient()
  const pagedKey = queryKeys.runs.list('p1', 1, 20)
  const infiniteKey = queryKeys.runs.infinite('p1', 20)
  const otherKey = queryKeys.runs.list('p2', 1, 20)

  client.setQueryData(pagedKey, page(run(2, 'running'), run(1, 'success')))
  client.setQueryData(infiniteKey, { pages: [page(run(2, 'running')), page(run(1, 'success'))], pageParams: [1, 2] })
  client.setQueryData(otherKey, page(run(2, 'running', 'p2')))

  const finished = { ...run(2, 'failed'), duration_seconds: 30 }

  updateRunInHistory(client, finished)

  assert.deepEqual(client.getQueryData<PaginatedRunHistory>(pagedKey)?.runs, [finished, run(1, 'success')])
  assert.deepEqual(
    client.getQueryData<{ pages: PaginatedRunHistory[] }>(infiniteKey)?.pages.map((entry) => entry.runs),
    [[finished], [run(1, 'success')]]
  )
  assert.equal(client.getQueryData<PaginatedRunHistory>(otherKey)?.runs[0]?.status, 'running')
})

test('a run missing from the cached pages leaves them untouched', () => {
  const client = new QueryClient()
  const key = queryKeys.runs.list('p1', 1, 20)
  const cached = page(run(1, 'success'))

  client.setQueryData(key, cached)
  updateRunInHistory(client, run(9, 'running'))

  assert.equal(client.getQueryData(key), cached)
})
