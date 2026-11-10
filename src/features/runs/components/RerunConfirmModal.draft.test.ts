import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'
import { runPresentationFixture } from '../../../testing/presentation'
import type { PipelineRun } from '../../../types/pipelines'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { RerunConfirmModal } = await import('./RerunConfirmModal')
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

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a rejected rerun hides its error when another mode is picked, without a new submit', async () => {
  const user = userEvent.setup({ document })
  const modes: string[] = []

  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client: new QueryClient() },
        createElement(RerunConfirmModal, {
          run,
          pipeline: undefined,
          onClose: () => undefined,
          onConfirm: async (mode) => {
            modes.push(mode)
            throw new Error('provider said no')
          },
        })
      )
    )
  )

  await user.click(await view().findByRole('radio', { name: /Re-run all jobs/ }))
  await user.click(view().getByRole('button', { name: 'Re-run all jobs' }))
  await view().findByText(/provider said no/)

  await user.click(view().getByRole('radio', { name: /Re-run failed jobs only/ }))
  await waitFor(() => assert.equal(view().queryAllByText(/provider said no/).length, 0))
  assert.deepEqual(modes, ['all'])

  await user.click(view().getByRole('radio', { name: /Re-run all jobs/ }))
  await view().findByText(/provider said no/)
})
