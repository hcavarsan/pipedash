import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineFixture } from '../../../testing/fixtures'
import { runDisplayFixture, runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'
import type { RunHistoryPage as RunHistoryPageData, RunHistoryQuery } from '../../../types/run_history'
import { pluginFixture } from '../../providers/components/AddProviderModal/pluginFixture'

GlobalRegistrator.register({ width: 393, height: 852 })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('data-virtual-item') ? 200 : 0
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService } = await import('../../../services/api')
const { wsClient } = await import('../../../services/websocket')
const { RunActionsProvider } = await import('./RunActionsContext')
const { RunHistoryPage } = await import('./RunHistoryPage')

const pipeline = pipelineFixture({ id: 'github__1__org/repo/build', provider_id: 0 })
const view = () => within(document.body)
let client = new QueryClient()

const runFixture = (runNumber: number): PipelineRun => ({
  id: `r${runNumber}`,
  pipeline_id: pipeline.id,
  run_number: runNumber,
  status: 'success',
  started_at: '2026-11-04T00:00:00Z',
  concluded_at: '2026-11-04T00:01:00Z',
  duration_seconds: 60,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: 'main',
  actor: null,
  ...runPresentationFixture(),
  display: runDisplayFixture({ title: `#${runNumber}` }),
})

function pageOf(query: RunHistoryQuery, runs: PipelineRun[], hasMore = false): RunHistoryPageData {
  return {
    runs,
    total_count: runs.length,
    has_more: hasMore,
    can_scan_deeper: false,
    is_complete: true,
    scan_notice: null,
    page: query.page,
    page_size: query.page_size,
    total_pages: hasMore ? 2 : 1,
    branches: [],
    actors: [],
  }
}

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  client.setQueryData(queryKeys.plugins.list(), [pluginFixture()])
  mock.method(apiService, 'getAvailablePlugins', async () => [pluginFixture()])
  mock.method(apiService, 'getPipelineMetricsConfig', async () => ({
    pipeline_id: pipeline.id,
    enabled: false,
    retention_days: 30,
    created_at: '2026-11-04T00:00:00Z',
    updated_at: '2026-11-04T00:00:00Z',
  }))
  mock.method(wsClient, 'listen', async () => () => undefined)
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  mock.restoreAll()
})

after(async () => {
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, name)
    }
  }
  await GlobalRegistrator.unregister()
})

function renderPage() {
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <ModalsProvider>
          <MemoryRouter>
            <RunActionsProvider pipeline={pipeline}>
              <RunHistoryPage
                pipeline={pipeline}
                tab="history"
                onTabChange={() => undefined}
                onBack={() => undefined}
              />
            </RunActionsProvider>
          </MemoryRouter>
        </ModalsProvider>
      </QueryClientProvider>
    </MantineProvider>
  )
}

for (const previous of ['runs', 'nothing'] as const) {
  test(`a filter change keeps the search input mounted after a result with ${previous}`, async () => {
    const gate = Promise.withResolvers<void>()
    const requests: RunHistoryQuery[] = []
    mock.method(apiService, 'fetchRunHistory', async (_id: string, query: RunHistoryQuery) => {
      requests.push(query)

      if (query.search) {
        await gate.promise

        return pageOf(query, [])
      }

      return pageOf(query, previous === 'runs' ? [runFixture(1)] : [])
    })
    renderPage()

    const search = await view().findByPlaceholderText(/^Search/)
    await userEvent.setup({ document }).type(search, 'x')
    await waitFor(() => assert.ok(requests.some((query) => query.search === 'x')))

    assert.equal(document.body.contains(search), true)
    assert.equal(view().getByPlaceholderText(/^Search/), search)
    await act(async () => gate.resolve())
    assert.equal(view().getByPlaceholderText(/^Search/), search)
  })
}

test('a rejected next page keeps the first page above an error and Retry requests the failed page', async () => {
  const pages: number[] = []
  let failing = true
  mock.method(apiService, 'fetchRunHistory', async (_id: string, query: RunHistoryQuery) => {
    pages.push(query.page)

    if (query.page === 2 && failing) {
      throw new Error('Page 2 unavailable')
    }

    return query.page === 1 ? pageOf(query, [runFixture(1)], true) : pageOf(query, [runFixture(2)])
  })
  renderPage()

  assert.ok(await view().findByText('Page 2 unavailable'))
  assert.ok(view().getByRole('button', { name: 'View details for run #1' }))
  assert.deepEqual(pages, [1, 2])

  failing = false
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Retry' }))

  assert.ok(await view().findByRole('button', { name: 'View details for run #2' }))
  assert.deepEqual(pages, [1, 2, 2])
  assert.equal(view().queryByText('Page 2 unavailable'), null)
  assert.ok(view().getByRole('button', { name: 'View details for run #1' }))
})

test('a failed first page shows the error with Retry and no rows', async () => {
  mock.method(apiService, 'fetchRunHistory', async () => {
    throw new Error('History unavailable')
  })
  renderPage()

  assert.ok(await view().findByText('History unavailable'))
  assert.ok(view().getByRole('button', { name: 'Retry' }))
  assert.equal(view().queryByRole('button', { name: /View details for run/ }), null)
})
