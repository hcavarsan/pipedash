import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import { runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'
import type { RunTarget } from '../../../types/run_action_batch'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { CancelActiveRunsModal } = await import('./CancelActiveRunsModal')
const { apiService } = await import('../../../services/api')
const { notifications } = await import('../../../lib/notifications')
const { useCancelActiveRuns } = await import('../../pipelines/hooks/useCancelActiveRuns')
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

const view = () => within(document.body)
const pipeline = pipelineFixture({ id: 'p1', name: 'Build', status: 'running', provider_type: 'github' })
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

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

function renderModal(onConfirm: () => Promise<unknown>, onClose: () => void) {
  return render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(CancelActiveRunsModal, {
          opened: true,
          scope: { label: 'Build', pipelines: [pipeline] },
          onClose,
          onConfirm,
        })
      )
    )
  )
}

test('a failed cancellation keeps the dialog open and a retry closes it', async (context) => {
  context.mock.method(apiService, 'getCancellationTargets', async () => ({
    groups: [{ pipeline, provider_name: 'GitHub', runs: [run] }],
    batches: [[{ pipeline_id: pipeline.id, run_number: run.run_number }]],
    failed_pipeline_ids: [],
    skipped: [],
    run_count: 1,
  }))
  const user = userEvent.setup({ document })
  let closed = 0
  let fail = true
  let attempts = 0
  renderModal(
    async () => {
      attempts += 1

      if (fail) {
        throw new Error('cancel failed')
      }
    },
    () => {
      closed += 1
    }
  )

  await user.click(await view().findByRole('button', { name: 'Cancel 1 run' }))
  await waitFor(() => assert.equal(attempts, 1))
  assert.equal(view().queryByRole('alert'), null)
  assert.equal(closed, 0)

  fail = false
  await user.click(view().getByRole('button', { name: 'Cancel 1 run' }))
  await waitFor(() => assert.equal(closed, 1))
})

const found = () => ({
  groups: [{ pipeline, provider_name: 'GitHub', runs: [run] }],
  batches: [[{ pipeline_id: pipeline.id, run_number: run.run_number }]],
  failed_pipeline_ids: [],
  skipped: [],
  run_count: 1,
})

test('a rejected discovery shows an error with Retry instead of an empty result, and a retry shows the runs', async (context) => {
  let failing = true
  const search = context.mock.method(apiService, 'getCancellationTargets', async () => {
    if (failing) {
      throw new Error('Providers unreachable')
    }

    return found()
  })
  renderModal(
    async () => undefined,
    () => undefined
  )

  assert.ok(await view().findByText('Providers unreachable'))
  assert.equal(view().queryByText(/Nothing is running/), null)
  assert.equal(view().queryByRole('button', { name: /^Cancel \d+ run/ }), null)

  failing = false
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Retry' }))

  assert.ok(await view().findByRole('button', { name: 'Cancel 1 run' }))
  assert.equal(view().queryByText('Providers unreachable'), null)
  assert.equal(search.mock.callCount(), 2)
})

test('a failed re-check removes the confirm action even though a count was found before', async (context) => {
  let failing = false
  context.mock.method(apiService, 'getCancellationTargets', async () => {
    if (failing) {
      throw new Error('Providers unreachable')
    }

    return found()
  })
  renderModal(
    async () => undefined,
    () => undefined
  )
  assert.ok(await view().findByRole('button', { name: 'Cancel 1 run' }))

  failing = true
  await act(async () => {
    await client.refetchQueries()
  })

  assert.ok(await view().findByText('Providers unreachable'))
  assert.equal(view().queryByRole('button', { name: /^Cancel \d+ run/ }), null)
})

function CancelLauncher() {
  const { cancelItem, dialog } = useCancelActiveRuns()
  const item = cancelItem('Build', [pipeline])

  return createElement(
    'div',
    null,
    createElement('button', { type: 'button', onClick: () => item?.onClick() }, 'Open'),
    dialog
  )
}

async function openCancelDialog(count = '1 run') {
  const user = userEvent.setup({ document })
  render(
    createElement(MantineProvider, null, createElement(QueryClientProvider, { client }, createElement(CancelLauncher)))
  )
  await user.click(view().getByRole('button', { name: 'Open' }))
  await user.click(await view().findByRole('button', { name: `Cancel ${count}` }))
}

function stubDiscovery(context: TestContext) {
  context.mock.method(apiService, 'getCancellationTargets', async () => ({
    groups: [{ pipeline, provider_name: 'GitHub', runs: [run] }],
    batches: [[{ pipeline_id: pipeline.id, run_number: run.run_number }]],
    failed_pipeline_ids: [],
    skipped: [],
    run_count: 1,
  }))
  return context.mock.method(notifications, 'show', () => 'id')
}

test('cancelling active runs shows one summary toast and refreshes the provider', async (context) => {
  const shown = stubDiscovery(context)
  const refresh = context.mock.method(apiService, 'refreshProvider', async () => ({
    provider_id: 1,
    state: 'ok',
    kind: null,
    message: null,
  }))
  context.mock.method(apiService, 'cancelPipelineRuns', async () => ({
    results: [{ target: { pipeline_id: 'p1', run_number: 11 }, pipeline_name: 'Build', error: null }],
    requested_count: 1,
    failed_count: 0,
  }))

  await openCancelDialog()

  await waitFor(() => assert.equal(refresh.mock.callCount(), 1))
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, 'Cancellation requested')
})

test('a rejected batch shows one error toast, no dialog alert, and the dialog can retry', async (context) => {
  const shown = stubDiscovery(context)
  const refresh = context.mock.method(apiService, 'refreshProvider', async () => undefined)
  const request = context.mock.method(apiService, 'cancelPipelineRuns', async () => {
    throw new Error('provider unreachable')
  })
  const errorToasts = () => shown.mock.calls.filter((call) => call.arguments[0]?.title === "Couldn't cancel runs")

  await openCancelDialog()

  await waitFor(() => assert.equal(errorToasts().length, 1))
  const confirm = view().getByRole('button', { name: 'Cancel 1 run' })
  await waitFor(() => assert.equal(confirm.hasAttribute('disabled'), false))
  assert.equal(view().queryByRole('alert'), null)
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(refresh.mock.callCount(), 0)

  await userEvent.setup({ document }).click(confirm)
  await waitFor(() => assert.equal(request.mock.callCount(), 2))
  await waitFor(() => assert.equal(errorToasts().length, 2))
  assert.equal(view().queryByRole('alert'), null)
})

test('confirmation cancels 1001 discovered runs in bounded batches and reports partial failures', async (context) => {
  const runs = Array.from({ length: 1001 }, (_, index) => ({ ...run, id: `r${index + 1}`, run_number: index + 1 }))
  const targets = runs.map(({ run_number }) => ({ pipeline_id: pipeline.id, run_number }))
  context.mock.method(apiService, 'getCancellationTargets', async () => ({
    groups: [{ pipeline, provider_name: 'Stub', runs }],
    batches: [targets.slice(0, 1000), targets.slice(1000)],
    failed_pipeline_ids: [],
    skipped: [],
    run_count: runs.length,
  }))
  const shown = context.mock.method(notifications, 'show', () => 'id')
  context.mock.method(apiService, 'refreshProvider', async () => undefined)
  const cancelled: number[] = []
  const sizes: number[] = []
  context.mock.method(apiService, 'cancelPipelineRuns', async (batch: RunTarget[]) => {
    sizes.push(batch.length)
    if (batch.length > 1000) {
      throw new Error('Cancel at most 1000 runs at a time')
    }
    const results = batch.map((target) => {
      const error = target.run_number === 13 ? 'run is locked' : null
      if (error === null) {
        cancelled.push(target.run_number)
      }
      return { target, pipeline_name: 'Build', error }
    })
    return {
      results,
      requested_count: batch.length,
      failed_count: results.filter(({ error }) => error !== null).length,
    }
  })

  await openCancelDialog('1,001 runs')

  await waitFor(() => assert.equal(cancelled.length, 1000))
  assert.deepEqual(
    cancelled,
    targets.filter(({ run_number }) => run_number !== 13).map(({ run_number }) => run_number)
  )
  assert.deepEqual(sizes, [1000, 1])
  assert.equal(shown.mock.callCount(), 1)
  assert.match(String(shown.mock.calls[0]?.arguments[0]?.message), /1000 of 1,001 runs/)
  await act(async () => undefined)
  await waitFor(() => assert.equal(view().queryByRole('button', { name: 'Cancel 1,001 runs' }) === null, true))
})
