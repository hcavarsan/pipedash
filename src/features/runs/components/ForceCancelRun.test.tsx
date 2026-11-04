import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import type { Pipeline, PipelineRun } from '../../../types/domain'
import { pluginFixture } from '../../providers/components/AddProviderModal/pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('../../../lib/notifications')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService } = await import('../../../services/api')
const { events } = await import('../../../services/service')
const { RunActionButtons } = await import('./RunActionControls')
const { RunActionsProvider } = await import('./RunActionsContext')
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
}
const plugin = pluginFixture()
let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  mock.method(apiService, 'getAvailablePlugins', async () => [
    { ...plugin, capabilities: { ...plugin.capabilities, force_cancel: true } },
  ])
  mock.method(apiService, 'getWorkflowRunDetails', async () => run)
  mock.method(apiService, 'refreshProvider', async () => ({
    provider_id: 1,
    state: 'ok',
    kind: null,
    message: null,
  }))
  mock.method(events, 'listen', async () => () => undefined)
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  mock.restoreAll()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function renderSurface(surface: 'history' | 'details', target: Pipeline = pipeline, targetRun = run) {
  const content =
    surface === 'history' ? (
      <RunActionsProvider pipeline={target}>
        <RunActionButtons run={targetRun} withDetails />
      </RunActionsProvider>
    ) : (
      createElement(WorkflowLogsModal, {
        opened: true,
        onClose: () => undefined,
        pipelineId: target.id,
        runNumber: targetRun.run_number,
        providerType: target.provider_type,
        pipeline: target,
        canViewLogs: false,
        canTrigger: false,
        canCancel: true,
      })
    )
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(QueryClientProvider, { client }, createElement(ModalsProvider, null, children))
    )

  return render(content, { wrapper })
}

for (const surface of ['history', 'details'] as const) {
  test(`${surface} force cancel identifies the run, dismisses without a request and confirms once`, async () => {
    const force = mock.method(apiService, 'forceCancelPipelineRun', async () => undefined)
    const cancel = mock.method(apiService, 'cancelPipelineRun', async () => undefined)
    const invalidations = mock.method(client, 'invalidateQueries')
    const user = userEvent.setup({ document })
    const view = within(document.body)
    renderSurface(surface)
    const name = surface === 'history' ? 'Force cancel run #7' : 'Force cancel'

    await user.click(await view.findByRole('button', { name }))
    const confirmation = await view.findByRole('dialog', { name: 'Force cancel run' })
    assert.match(confirmation.textContent ?? '', /org\/repo/)
    assert.match(confirmation.textContent ?? '', /feature\/cleanup/)
    assert.match(confirmation.textContent ?? '', /run #7 only after normal cancellation fails/)
    assert.match(confirmation.textContent ?? '', /GitHub bypasses conditions such as always\(\)/)
    assert.equal(force.mock.callCount(), 0)
    await user.click(within(confirmation).getByRole('button', { name: 'Keep running' }))
    await waitFor(() => assert.ok(!view.queryByRole('dialog', { name: 'Force cancel run' })))
    assert.equal(force.mock.callCount(), 0)

    await user.click(view.getByRole('button', { name }))
    const confirmed = await view.findByRole('dialog', { name: 'Force cancel run' })
    await user.click(within(confirmed).getByRole('button', { name: /^Force cancel$/ }))
    await waitFor(() => assert.equal(force.mock.callCount(), 1))
    assert.deepEqual(force.mock.calls[0]?.arguments, [pipeline.id, 7])
    assert.equal(cancel.mock.callCount(), 0)
    for (const queryKey of [
      queryKeys.runs.detail(pipeline.id, 7),
      queryKeys.runs.list(pipeline.id),
      queryKeys.pipelines.all,
    ]) {
      assert.ok(
        invalidations.mock.calls.some(
          (call) => JSON.stringify(call.arguments[0]?.queryKey) === JSON.stringify(queryKey)
        )
      )
    }
  })

  test(`${surface} reports a rejected force request without retrying or normal cancellation`, async () => {
    const force = mock.method(apiService, 'forceCancelPipelineRun', async () => {
      throw new Error('GitHub refused force cancellation')
    })
    const cancel = mock.method(apiService, 'cancelPipelineRun', async () => undefined)
    const shown = mock.method(notifications, 'show')
    const user = userEvent.setup({ document })
    const view = within(document.body)
    renderSurface(surface)
    await user.click(
      await view.findByRole('button', { name: surface === 'history' ? 'Force cancel run #7' : 'Force cancel' })
    )
    const confirmation = await view.findByRole('dialog', { name: 'Force cancel run' })
    await user.click(within(confirmation).getByRole('button', { name: /^Force cancel$/ }))
    await waitFor(() =>
      assert.ok(shown.mock.calls.some((call) => call.arguments[0]?.title === "Couldn't force cancel run"))
    )
    assert.equal(force.mock.callCount(), 1)
    assert.equal(cancel.mock.callCount(), 0)
    assert.ok(shown.mock.calls.some((call) => call.arguments[0]?.message === 'GitHub refused force cancellation'))
  })

  for (const scenario of ['read-only', 'other provider', 'unsupported', 'pending', 'finished'] as const) {
    test(`${surface} hides force cancel for ${scenario} runs`, async () => {
      const target = {
        ...pipeline,
        provider_type: scenario === 'other provider' ? 'gitlab' : 'github',
        metadata: { read_only: scenario === 'read-only' },
      }
      const targetRun: PipelineRun = {
        ...run,
        status: scenario === 'pending' ? 'pending' : scenario === 'finished' ? 'success' : 'running',
      }
      mock.method(apiService, 'getWorkflowRunDetails', async () => targetRun)
      mock.method(apiService, 'getAvailablePlugins', async () => [
        {
          ...plugin,
          provider_type: target.provider_type,
          capabilities: { ...plugin.capabilities, force_cancel: scenario !== 'unsupported' },
        },
      ])
      const view = within(document.body)
      renderSurface(surface, target, targetRun)
      await waitFor(() => assert.equal(client.getQueryState(queryKeys.plugins.list())?.status, 'success'))
      if (surface === 'details') {
        await waitFor(() =>
          assert.equal(client.getQueryState(queryKeys.runs.detail(pipeline.id, 7))?.status, 'success')
        )
      }
      assert.equal(view.queryByRole('button', { name: /Force cancel/ }), null)
    })
  }
}

for (const surface of ['history', 'details'] as const) {
  for (const rejected of [false, true]) {
    test(`${surface} keeps force cancel separate after normal cancellation ${rejected ? 'fails' : 'is accepted'}`, async () => {
      mock.method(apiService, 'cancelPipelineRun', async () => {
        if (rejected) {
          throw new Error('Normal cancellation failed')
        }
      })
      const force = mock.method(apiService, 'forceCancelPipelineRun', async () => undefined)
      const user = userEvent.setup({ document })
      const view = within(document.body)
      renderSurface(surface)
      await user.click(
        await view.findByRole('button', { name: surface === 'history' ? 'Cancel run #7' : 'Stop workflow' })
      )
      const confirmation = await view.findByRole('dialog', { name: 'Cancel run' })
      await user.click(within(confirmation).getByRole('button', { name: /^Cancel run$/ }))
      await waitFor(() => assert.ok(!view.queryByRole('dialog', { name: 'Cancel run' })))
      const button = await view.findByRole('button', {
        name: surface === 'history' ? 'Force cancel run #7' : 'Force cancel',
      })
      await waitFor(() => assert.equal(button.hasAttribute('disabled'), false))
      assert.equal(force.mock.callCount(), 0)
    })
  }
}
