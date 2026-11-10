import '../../../testing/registerDom'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { createElement } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { notifications } from '../../../lib/notifications'
import { apiService } from '../../../services/api'
import { tauriService } from '../../../services/tauri'
import { pipelineFixture } from '../../../testing/fixtures'
import { runPresentationFixture } from '../../../testing/presentation'
import type { RunHistoryPage, RunHistoryQuery } from '../../../types/run_history'
import { useRunHistoryPageData } from './useRunHistoryPageData'

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(MemoryRouter, null, createElement(QueryClientProvider, { client }, children))
const pipeline = pipelineFixture({ provider_id: 0 })

function history(query: RunHistoryQuery): RunHistoryPage {
  const complete = (query.scan_depth ?? 1) >= 2
  const runNumber = (query.page - 1) * query.page_size + 1

  return {
    runs: complete
      ? [
          {
            id: `run-${runNumber}`,
            pipeline_id: pipeline.id,
            run_number: runNumber,
            status: 'success',
            started_at: '2026-01-01T00:00:00Z',
            concluded_at: '2026-01-01T00:01:00Z',
            duration_seconds: 60,
            logs_url: '',
            commit_sha: null,
            commit_message: null,
            branch: null,
            actor: null,
            ...runPresentationFixture(),
          },
        ]
      : [],
    total_count: complete ? query.page_size * 3 : 0,
    has_more: complete && query.page < 3,
    can_scan_deeper: !complete,
    is_complete: complete,
    scan_notice: complete
      ? null
      : { title: 'More history needed', message: 'Scan more runs.', action_label: 'Load more history' },
    page: query.page,
    page_size: query.page_size,
    total_pages: complete ? 3 : 0,
    branches: [],
    actors: [],
  }
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(() => GlobalRegistrator.unregister())

test('completed history stays complete across forward and backward page navigation', async (context) => {
  const requests: RunHistoryQuery[] = []
  const fetchHistory = async (_pipelineId: string, query: RunHistoryQuery) => {
    requests.push(query)
    return history(query)
  }
  context.mock.method(apiService, 'fetchRunHistory', fetchHistory)
  context.mock.method(tauriService, 'fetchRunHistory', fetchHistory)
  const { result } = renderHook(() => useRunHistoryPageData(pipeline), { wrapper })
  act(() => result.current.setSortOverride({ columnAccessor: 'run_number', direction: 'asc' }))

  await waitFor(() => assert.equal(result.current.hasMore, true))
  act(() => result.current.fetchMore())
  await waitFor(() => assert.equal(result.current.runs[0]?.run_number, 1))

  act(() => result.current.setFilter('page', 2))
  await waitFor(() => assert.equal(result.current.runs[0]?.run_number, result.current.pageSize + 1))
  assert.equal(result.current.scanNotice, null)
  assert.equal(result.current.isComplete, true)
  assert.equal(requests.at(-1)?.scan_depth, 2)

  act(() => result.current.setFilter('page', 1))
  await waitFor(() => assert.equal(result.current.filters.page, 1))
  await act(async () => {
    await result.current.refetch()
  })
  assert.equal(result.current.runs[0]?.run_number, 1)
  assert.equal(result.current.isComplete, true)
  assert.equal(requests.at(-1)?.page, 1)
  assert.equal(requests.at(-1)?.scan_depth, 2)
})

test('a rejected history refresh rejects the promise and shows one error toast', async (context) => {
  let failing = false
  const fetchHistory = async (_pipelineId: string, query: RunHistoryQuery) => {
    if (failing) {
      throw new Error('History unavailable')
    }

    return history(query)
  }
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'fetchRunHistory', fetchHistory)
  context.mock.method(tauriService, 'fetchRunHistory', fetchHistory)
  const { result } = renderHook(() => useRunHistoryPageData(pipeline), { wrapper })

  await waitFor(() => assert.equal(result.current.isLoading, false))
  failing = true

  let outcome: unknown
  await act(async () => {
    outcome = await result.current.refresh().then(
      () => 'resolved',
      (error: unknown) => error
    )
  })

  assert.ok(outcome instanceof Error)
  assert.equal(outcome.message, 'History unavailable')
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, "Couldn't refresh run history")
})
