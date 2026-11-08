import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineFixture } from '../../../testing/fixtures'
import { pluginFixture } from '../../providers/components/AddProviderModal/pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: { invoke: async (command: string) => (command === 'get_plugins' ? [] : null) },
})

const { cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { queryKeys } = await import('../../../lib/queryKeys')
const { RunActionsProvider } = await import('./RunActionsContext')
const { RunHistoryHeader } = await import('./RunHistoryHeader')

const view = () => within(document.body)

function mount(overrides: Parameters<typeof pipelineFixture>[0]) {
  const pipeline = pipelineFixture({ provider_type: 'stub', ...overrides })
  const client = new QueryClient()
  client.setQueryData(queryKeys.plugins.list(), [pluginFixture({ provider_type: 'stub' })])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <RunActionsProvider pipeline={pipeline}>
            <RunHistoryHeader
              pipeline={pipeline}
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

test('omits the current state block without a state message', () => {
  mount({ status: 'success', resource_state: { label: 'Up' } })
  assert.equal(view().queryByText('Up'), null)
})

test('omits the current state block without a resource state', () => {
  mount({ status: 'success' })
  assert.equal(view().queryByText('Success'), null)
})
