import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import type { PipelineRun, PluginMetadata } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { CancelActiveRunsModal } = await import('./CancelActiveRunsModal')
const { apiService } = await import('../../../services/api')
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
}
const plugin: PluginMetadata = {
  name: 'GitHub',
  provider_type: 'github',
  version: '1.0.0',
  description: '',
  author: null,
  icon: null,
  config_schema: { fields: [] },
  table_schema: { tables: [] },
  capabilities: {
    pipelines: true,
    pipeline_runs: true,
    trigger: true,
    cancel: true,
    force_cancel: false,
    agents: false,
    artifacts: false,
    queues: false,
    custom_tables: false,
    retains_run_history: true,
    logs: true,
    sign_in: false,
  },
  required_permissions: [],
  features: [],
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
  context.mock.method(apiService, 'getAvailablePlugins', async () => [plugin])
  context.mock.method(apiService, 'fetchRunHistory', async () => ({
    runs: [run],
    total_count: 1,
    has_more: false,
    is_complete: true,
    page: 1,
    page_size: 20,
    total_pages: 1,
  }))
  const user = userEvent.setup({ document })
  let closed = 0
  let fail = true
  renderModal(
    async () => {
      if (fail) {
        throw new Error('cancel failed')
      }
    },
    () => {
      closed += 1
    }
  )

  await user.click(await view().findByRole('button', { name: 'Cancel 1 run' }))
  await waitFor(() => assert.ok(view().queryByRole('alert')))
  assert.equal(closed, 0)

  fail = false
  await user.click(view().getByRole('button', { name: 'Cancel 1 run' }))
  await waitFor(() => assert.equal(closed, 1))
})
