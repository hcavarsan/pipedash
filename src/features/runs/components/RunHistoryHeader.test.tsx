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

test('names the pipeline and leaves its state and message to the rows and run details', () => {
  mount({
    name: 'checkout',
    status: 'degraded',
    resource_state: { label: 'Progressing', message: 'Waiting for rollout' },
  })

  assert.ok(view().getAllByText('checkout').length > 0)
  assert.equal(view().queryByText('Progressing'), null)
  assert.equal(view().queryByText('Waiting for rollout'), null)
})
