import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { runActionsFixture, runPresentationFixture } from '../../../testing/presentation'
import type { RunLogNode } from '../../../types/logs'
import type { PipelineRun } from '../../../types/pipelines'
import { pluginFixture } from '../../providers/components/AddProviderModal/pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService } = await import('../../../services/api')
const { wsClient } = await import('../../../services/websocket')
const { RerunJobBar } = await import('./RerunJobBar')

const pipelineId = 'stub__1__acme/app/42'
const plugin = pluginFixture()
const node: RunLogNode = {
  id: '502',
  parent_id: null,
  kind: 'job',
  name: 'deploy',
  status: 'failed',
  started_at: null,
  finished_at: null,
  duration_seconds: null,
  log: 'ready',
  web_url: null,
  depth: 0,
  navigable: true,
}

const runWith = (allowed: boolean): PipelineRun => ({
  id: 'r7',
  pipeline_id: pipelineId,
  run_number: 7,
  status: 'failed',
  started_at: '2026-11-04T00:00:00Z',
  concluded_at: '2026-11-04T00:05:00Z',
  duration_seconds: 300,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: 'main',
  actor: null,
  ...runPresentationFixture(),
  actions: runActionsFixture({ rerun_job: { allowed, reason: null } }),
})

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  mock.method(apiService, 'getAvailablePlugins', async () => [plugin])
  mock.method(wsClient, 'listen', async () => () => undefined)
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  mock.restoreAll()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function renderBar() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(QueryClientProvider, { client }, createElement(ModalsProvider, null, children))
    )

  return render(<RerunJobBar pipelineId={pipelineId} runNumber={7} providerType={plugin.provider_type} node={node} />, {
    wrapper,
  })
}

test('confirming the named job reruns only that job and refreshes the run and its logs', async () => {
  mock.method(apiService, 'getWorkflowRunDetails', async () => runWith(true))
  const rerunJob = mock.method(apiService, 'rerunJob', async () => undefined)
  const invalidations = mock.method(client, 'invalidateQueries')
  const user = userEvent.setup({ document })
  const view = within(document.body)
  renderBar()

  await user.click(await view.findByRole('button', { name: 'Re-run job' }))
  const dialog = await view.findByRole('dialog', { name: 'Re-run job' })
  assert.ok(within(dialog).getByText('deploy'))
  assert.ok(within(dialog).getByText(plugin.presentation.rerun_job_confirmation))
  assert.equal(rerunJob.mock.callCount(), 0)

  await user.click(within(dialog).getByRole('button', { name: 'Re-run job' }))

  await waitFor(() => assert.equal(rerunJob.mock.callCount(), 1))
  assert.deepEqual(rerunJob.mock.calls[0]?.arguments, [pipelineId, 7, '502'])
  await waitFor(() =>
    assert.ok(
      invalidations.mock.calls.some(
        (call) => JSON.stringify(call.arguments[0]?.queryKey) === JSON.stringify(queryKeys.runLogs.tree(pipelineId, 7))
      )
    )
  )
})

test('a run the backend does not allow shows no rerun job action', async () => {
  const details = mock.method(apiService, 'getWorkflowRunDetails', async () => runWith(false))
  const view = within(document.body)
  renderBar()

  await waitFor(() => assert.equal(details.mock.callCount(), 1))
  assert.equal(view.queryByRole('button', { name: 'Re-run job' }), null)
})
