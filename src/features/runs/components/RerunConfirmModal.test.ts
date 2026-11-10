import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'
import { pipelineFixture } from '../../../testing/fixtures'
import { pluginPresentationFixture, runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun, TriggerDraft } from '../../../types/pipelines'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { RerunConfirmModal } = await import('./RerunConfirmModal')
const { apiService } = await import('../../../services/api')
const view = () => within(document.body)

const run: PipelineRun = {
  id: 'r1',
  pipeline_id: 'p1',
  run_number: 7,
  status: 'failed',
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

const draft: TriggerDraft = {
  parameters: [],
  values: {},
  fields: [{ label: 'Run', value: '#7' }],
  errors: {},
  availability: { allowed: true, reason: null },
  copy: { ...pluginPresentationFixture().trigger, label: 'Re-run', compact_label: 'Re-run' },
}

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a rejected rerun keeps the dialog open with its mode and shows the error, a retry closes it', async () => {
  mock.method(apiService, 'preparePipelineTrigger', async () => draft)
  const user = userEvent.setup({ document })
  const modes: string[] = []
  let closes = 0
  let fail = true

  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client: new QueryClient() },
        createElement(RerunConfirmModal, {
          run,
          pipeline: pipelineFixture(),
          onClose: () => {
            closes += 1
          },
          onConfirm: async (mode) => {
            modes.push(mode)

            if (fail) {
              throw new Error('provider said no')
            }
          },
        })
      )
    )
  )

  await view().findByText(/Run:/)
  await user.click(await view().findByRole('radio', { name: /Re-run all jobs/ }))
  await user.click(view().getByRole('button', { name: 'Re-run all jobs' }))

  await view().findByText(/provider said no/)
  assert.equal(closes, 0)

  fail = false
  await user.click(view().getByRole('button', { name: 'Re-run all jobs' }))
  await waitFor(() => assert.equal(closes, 1))
  assert.deepEqual(modes, ['all', 'all'])
})
