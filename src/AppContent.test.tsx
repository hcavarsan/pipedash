import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import {
  pipelineViewFixture,
  pipelineViewPageFixture,
  providerSummaryFixture,
  viewPipelineFixture,
} from './testing/fixtures'
import { sectionPresentationFixture } from './testing/presentation'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })

const providers = [providerSummaryFixture({ id: 1, name: 'GitHub' }), providerSummaryFixture({ id: 2, name: 'GitLab' })]
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
      provider_ids: [1, 2],
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
let filteredGate: Promise<void> | null = null

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
  __TAURI_INTERNALS__: {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback: () => 1,
    invoke: async (command: string, args?: { providerId?: number | null }) => {
      switch (command) {
        case 'list_providers':
          return providers
        case 'get_cached_pipelines':
          if (args?.providerId) {
            await filteredGate
          }
          return [viewPipelineFixture({ id: 'p1', name: 'Nightly build' })]
        case 'list_pipeline_views':
          return [page.view]
        case 'get_pipeline_view_page':
          return page
        case 'get_vault_status':
          return { requires_password: false, is_unlocked: true }
        case 'get_available_plugins':
        case 'list_pins':
          return []
        case 'set_notification_preferences':
          return null
        case 'get_update_status':
          return {
            phase: 'idle',
            current_version: '0.13.0',
            version: null,
            notes: null,
            downloaded: 0,
            total: null,
            message: '',
          }
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

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useSearchParams } = await import('react-router-dom')
const { AppContent } = await import('./AppContent')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

notifyManager.setNotifyFunction((callback) => act(callback))

function FilterByProvider() {
  const [, setSearchParams] = useSearchParams()

  return (
    <button type="button" onClick={() => setSearchParams({ provider: '2' })}>
      Filter to GitLab
    </button>
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  filteredGate = null
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('changing the provider filter while the cached pipelines read is pending keeps the pipeline list', async () => {
  const gate = Promise.withResolvers<void>()
  filteredGate = gate.promise
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/pipelines']}>
          <FilterByProvider />
          <AppContent />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await view().findByText('Nightly build')

  await userEvent.setup({ document, delay: null }).click(view().getByRole('button', { name: 'Filter to GitLab' }))

  assert.equal(view().queryByText('Loading pipelines…'), null)
  assert.ok(view().getByText('Nightly build'))
  await act(async () => gate.resolve())
  assert.ok(view().getByText('Nightly build'))
})
