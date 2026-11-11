import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactNode } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import { runActionsFixture, runPresentationFixture } from '../../../testing/presentation'
import type { RunLogNode, RunLogTree } from '../../../types/logs'
import type { PipelineRun } from '../../../types/pipelines'
import { pluginFixture } from '../../providers/components/AddProviderModal/pluginFixture'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('data-virtual-item') ? 52 : 0
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
const { WorkflowLogsModal } = await import('./WorkflowLogsModal')

const pipeline = pipelineFixture({ id: 'github__1__org/repo/build', branch: 'main', status: 'running' })
const run: PipelineRun = {
  id: 'r7',
  pipeline_id: pipeline.id,
  run_number: 7,
  status: 'running',
  started_at: '2026-11-04T00:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: 'feature/cleanup',
  actor: null,
  ...runPresentationFixture(),
  actions: runActionsFixture(),
}
const compile: RunLogNode = {
  id: '111',
  parent_id: null,
  kind: 'job',
  name: 'Compile',
  status: 'success',
  started_at: null,
  finished_at: null,
  duration_seconds: 5,
  log: 'ready',
  web_url: null,
  depth: 0,
  navigable: true,
}
const tree: RunLogTree = {
  nodes: [compile],
  default_node_id: '111',
  complete: true,
  groups: [{ stage: null, node_ids: ['111'], failed_first_node_ids: ['111'] }],
  step_ids_by_node: {},
}
const view = () => within(document.body)
let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  mock.method(apiService, 'getAvailablePlugins', async () => [pluginFixture()])
  mock.method(apiService, 'getWorkflowRunDetails', async () => run)
  mock.method(apiService, 'getRunLogTree', async () => tree)
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

function renderModal() {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <ModalsProvider>
          <MemoryRouter>{children}</MemoryRouter>
        </ModalsProvider>
      </QueryClientProvider>
    </MantineProvider>
  )

  return render(
    <WorkflowLogsModal
      opened
      onClose={() => undefined}
      pipelineId={pipeline.id}
      runNumber={7}
      pipeline={pipeline}
      canViewLogs
    />,
    { wrapper }
  )
}

test('a failed run-details refetch keeps the loaded run and actions on screen with the error and Retry', async () => {
  let failing = false
  mock.method(apiService, 'getWorkflowRunDetails', async () => {
    if (failing) {
      throw new Error('Run unavailable')
    }

    return run
  })
  renderModal()
  assert.ok(await view().findByText(/Details update live/))

  failing = true
  await act(async () => {
    await client.refetchQueries({ queryKey: queryKeys.runs.detail(pipeline.id, 7) })
  })

  assert.ok(await view().findByText('Run unavailable'))
  assert.ok(view().getByText(/Details update live/))
  assert.ok(view().getByRole('button', { name: /^Cancel run #/ }))

  failing = false
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.ok(!view().queryByText('Run unavailable')))
  assert.ok(view().getByText(/Details update live/))
})

test('mobile jobs show loading, then an error with Retry, then the jobs, and the details stay visible', async () => {
  const pending = Promise.withResolvers<RunLogTree>()
  let attempts = 0
  mock.method(apiService, 'getRunLogTree', () => {
    attempts += 1

    return attempts === 1 ? pending.promise : Promise.resolve(tree)
  })
  renderModal()

  assert.ok(await view().findByText('Loading jobs…'))
  assert.ok(await view().findByText(/Details update live/))
  assert.ok(view().getByRole('button', { name: /^Cancel run #/ }))

  await act(async () => pending.reject(new Error('Jobs unavailable')))

  assert.ok(await view().findByText('Jobs unavailable'))
  assert.equal(view().queryByText('Loading jobs…'), null)
  assert.ok(view().getByText(/Details update live/))
  assert.ok(view().getByRole('button', { name: /^Cancel run #/ }))

  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Retry' }))

  assert.ok(await view().findByText('Compile'))
  assert.equal(view().queryByText('Jobs unavailable'), null)
  assert.ok(view().getByText(/Details update live/))
})
