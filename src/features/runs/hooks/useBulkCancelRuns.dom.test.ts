import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import { runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('../../../lib/notifications')
const { apiService } = await import('../../../services/api')
const { useBulkCancelRuns } = await import('./useBulkCancelRuns')

const pipeline = pipelineFixture({ id: 'p1', name: 'Build', status: 'running' })
const run: PipelineRun = {
  id: 'r1',
  pipeline_id: 'p1',
  run_number: 11,
  status: 'running',
  started_at: '2026-10-25T00:00:00Z',
  concluded_at: null,
  duration_seconds: null,
  logs_url: '',
  commit_sha: null,
  commit_message: null,
  branch: 'main',
  actor: null,
  ...runPresentationFixture(),
}
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  mock.restoreAll()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function Harness() {
  const bulk = useBulkCancelRuns({ pipeline, page: 1, runs: [run], enabled: true, cancellingRunNumber: null })

  return createElement(
    'div',
    null,
    createElement('p', { 'data-testid': 'selected' }, String(bulk.selectedRuns.length)),
    createElement('button', { type: 'button', onClick: () => bulk.selectRuns([run]) }, 'Select'),
    createElement('button', { type: 'button', onClick: bulk.cancelSelected }, 'Cancel selected')
  )
}

async function confirmBulkCancel() {
  const user = userEvent.setup({ document })
  const view = within(document.body)
  render(
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(QueryClientProvider, { client }, createElement(ModalsProvider, null, createElement(Harness)))
    )
  )

  await user.click(view.getByRole('button', { name: 'Select' }))
  assert.equal(view.getByTestId('selected').textContent, '1')
  await user.click(view.getByRole('button', { name: 'Cancel selected' }))
  const dialog = await view.findByRole('dialog')
  await user.click(within(dialog).getByRole('button', { name: /^Cancel run$/ }))

  return view
}

test('a rejected bulk cancel shows one error toast and keeps the selection', async () => {
  const request = mock.method(apiService, 'cancelPipelineRuns', async () => {
    throw new Error('provider unreachable')
  })
  const shown = mock.method(notifications, 'show', () => 'id')

  const view = await confirmBulkCancel()

  await waitFor(() => assert.equal(request.mock.callCount(), 1))
  await waitFor(() =>
    assert.equal(shown.mock.calls.filter((call) => call.arguments[0]?.title === "Couldn't cancel runs").length, 1)
  )
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(view.getByTestId('selected').textContent, '1')
})

test('a successful bulk cancel clears the selection', async () => {
  mock.method(apiService, 'cancelPipelineRuns', async () => ({
    results: [{ target: { pipeline_id: 'p1', run_number: 11 }, pipeline_name: 'Build', error: null }],
    requested_count: 1,
    failed_count: 0,
  }))
  const shown = mock.method(notifications, 'show', () => 'id')

  const view = await confirmBulkCancel()

  await waitFor(() => assert.equal(view.getByTestId('selected').textContent, '0'))
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, 'Cancellation requested')
})
