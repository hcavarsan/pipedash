import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { PipelineRun } from '../../types'

if (process.env.PIPEDASH_RERUN_TEST !== '1') {
  test('rerun confirmation behavior runs in its own DOM process', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_RERUN_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 1400, height: 900 })

  const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
  const { userEvent } = await import('@testing-library/user-event')
  const { MantineProvider } = await import('@mantine/core')
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
    metadata: { can_rerun_failed_jobs: true },
  }

  afterEach(async () => {
    await act(async () => cleanup())
  })

  after(async () => {
    await GlobalRegistrator.unregister()
  })

  test('a rejected rerun keeps the dialog open with its mode and shows the error, a retry closes it', async () => {
    const user = userEvent.setup({ document })
    const modes: string[] = []
    let closes = 0
    let fail = true

    render(
      createElement(
        MantineProvider,
        null,
        createElement(RerunConfirmModal, {
          run,
          pipeline: undefined,
          providerName: 'GitHub',
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

    await user.click(await view().findByRole('radio', { name: /Re-run all jobs/ }))
    await user.click(view().getByRole('button', { name: 'Re-run all jobs' }))

    await view().findByText(/provider said no/)
    assert.equal(closes, 0)

    fail = false
    await user.click(view().getByRole('button', { name: 'Re-run all jobs' }))
    await waitFor(() => assert.equal(closes, 1))
    assert.deepEqual(modes, ['all', 'all'])
  })
}
