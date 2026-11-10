import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import {
  pipelineViewFixture,
  pipelineViewPageFixture,
  providerSummaryFixture,
  viewPipelineFixture,
} from '../../../testing/fixtures'
import { sectionPresentationFixture } from '../../../testing/presentation'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })

const provider = providerSummaryFixture({ id: 1, name: 'GitHub' })
const page = pipelineViewPageFixture({
  view: pipelineViewFixture({ count: 1 }),
  total: 1,
  section_count: 1,
  sections: [
    {
      id: 'all',
      kind: 'none',
      title: 'All',
      organization: null,
      provider_ids: [1],
      status: null,
      status_pipeline_id: null,
      last_run: null,
      pending_fetch: false,
      presentation: sectionPresentationFixture(),
      pipelines: [viewPipelineFixture({ id: 'p1', name: 'Nightly build' })],
      freshness: { state: 'fresh', stale_at: null },
    },
  ],
})
let pageFailure: Error | null = null
let pageGate: Promise<void> | null = null

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    transformCallback: () => 1,
    invoke: async (command: string) => {
      switch (command) {
        case 'list_pipeline_views':
          return [page.view]
        case 'get_pipeline_view_page':
          await pageGate
          if (pageFailure) {
            throw pageFailure
          }
          return page
        case 'get_available_plugins':
        case 'list_pins':
          return []
        case 'plugin:event|listen':
          return 1
        case 'plugin:event|unlisten':
          return
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { UnifiedPipelinesView } = await import('./UnifiedPipelinesView')
const { queryKeys } = await import('../../../lib/queryKeys')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

notifyManager.setNotifyFunction((callback) => act(callback))

function mount() {
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/pipelines']}>
          <UnifiedPipelinesView
            providers={[provider]}
            onRetry={() => {
              void client.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
            }}
            onTrigger={() => {}}
            onViewHistory={() => {}}
            onViewRun={() => {}}
          />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  pageFailure = null
  pageGate = null
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('a failed background refetch keeps the loaded pipelines and Retry shows busy while it runs', async () => {
  mount()
  await view().findByText('Nightly build')

  pageFailure = new Error('Backend unreachable')
  await act(async () => {
    await client.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
  })
  await view().findByText("Couldn't load pipelines")
  assert.ok(view().getByText('Backend unreachable'))
  assert.ok(view().getByText('Nightly build'))

  const gate = Promise.withResolvers<void>()
  pageGate = gate.promise
  pageFailure = null
  await userEvent.setup({ document, delay: null }).click(view().getByRole('button', { name: 'Retry' }))
  await waitFor(() => assert.equal(view().getByRole('button', { name: 'Retry' }).hasAttribute('data-loading'), true))
  assert.ok(view().getByText('Nightly build'))

  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(view().queryByText("Couldn't load pipelines"), null))
  assert.ok(view().getByText('Nightly build'))
})

test('a first load that fails blocks with the error and Retry', async () => {
  pageFailure = new Error('Backend unreachable')
  mount()
  await view().findByText("Couldn't load pipelines")
  assert.equal(view().queryByText('Nightly build'), null)
  assert.ok(view().getByRole('button', { name: 'Retry' }))
})
