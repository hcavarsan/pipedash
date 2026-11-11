import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pluginFixture } from '../../features/providers/components/AddProviderModal/pluginFixture'
import { queryKeys } from '../../lib/queryKeys'
import { pipelineFixture } from '../../testing/fixtures'
import { runPresentationFixture } from '../../testing/presentation'
import type { PipelineRun } from '../../types/pipelines'

GlobalRegistrator.register({ width: 393, height: 852 })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('data-virtual-item') ? 64 : 0
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { IosRunList } = await import('./IosRunList')
const { RunActionsProvider } = await import('../../features/runs/components/RunActionsContext')
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

const view = () => within(document.body)
const NOW = Date.parse('2026-10-25T12:00:00Z')

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await act(async () => cleanup())
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, name)
    }
  }
  await GlobalRegistrator.unregister()
})

test('relative start times keep advancing while the list props stay unchanged', async (context) => {
  context.mock.timers.enable({ apis: ['Date', 'setInterval'], now: NOW })
  const run: PipelineRun = {
    id: 'r1',
    pipeline_id: 'p1',
    run_number: 3,
    status: 'success',
    started_at: new Date(NOW).toISOString(),
    concluded_at: null,
    duration_seconds: null,
    logs_url: '',
    commit_sha: null,
    commit_message: null,
    branch: null,
    actor: null,
    ...runPresentationFixture(),
  }
  const runs = [run]
  const props = {
    runs,
    label: '1 run',
    selecting: false,
    selectedNumbers: [],
    isSelectable: () => false,
    onToggle: () => undefined,
  }

  client.setQueryData(queryKeys.plugins.list(), [pluginFixture({ provider_type: 'stub' })])

  render(
    <MantineProvider>
      <QueryClientProvider client={client}>
        <RunActionsProvider pipeline={pipelineFixture({ id: 'p1' })}>
          <IosRunList {...props} />
        </RunActionsProvider>
      </QueryClientProvider>
    </MantineProvider>
  )

  assert.ok(await view().findByText(/now/))
  act(() => context.mock.timers.tick(5 * 60_000))
  await waitFor(() => assert.ok(view().getByText(/5m ago/)))
})
