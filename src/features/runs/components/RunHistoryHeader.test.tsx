import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: { invoke: async (command: string) => (command === 'get_plugins' ? [] : null) },
})

const { cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { RunActionsProvider } = await import('./RunActionsContext')
const { RunHistoryHeader } = await import('./RunHistoryHeader')

const view = () => within(document.body)

function mount(overrides: Parameters<typeof pipelineFixture>[0]) {
  const pipeline = pipelineFixture({ provider_type: 'argocd', ...overrides })
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <RunActionsProvider pipeline={pipeline}>
            <RunHistoryHeader
              pipeline={pipeline}
              isLoadingPipeline={false}
              showCustomize={false}
              customizationDisabled={false}
              onBack={() => undefined}
              onCustomize={() => undefined}
            />
          </RunActionsProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

test('shows the current state block for a non-Flux pipeline with a resource state', () => {
  mount({ status: 'degraded', resource_state: { label: 'Progressing', message: 'Waiting for rollout' } })
  assert.ok(view().getByText('Progressing'))
  assert.ok(view().getByText('Waiting for rollout'))
})

test('omits the current state block without a resource state', () => {
  mount({ status: 'success' })
  assert.equal(view().queryByText('Success'), null)
})
