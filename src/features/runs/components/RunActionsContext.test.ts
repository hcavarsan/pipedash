import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { pipelineFixture } from '../../../testing/fixtures'
import type { PipelineRun } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const backend: { failing: boolean; calls: string[] } = { failing: true, calls: [] }

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string) => {
      backend.calls.push(command)

      if (command === 'get_plugins') {
        return []
      }
      if (command.startsWith('rerun') && backend.failing) {
        throw new Error('provider refused')
      }

      return null
    },
  },
})

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { notifications } = await import('@mantine/notifications')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { RunActionsProvider, useRunActions } = await import('./RunActionsContext')

const pipeline = pipelineFixture({ id: 'p1', provider_type: 'github' })
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
}

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  backend.failing = true
  backend.calls = []
})

afterEach(async () => {
  await act(async () => cleanup())
  notifications.clean()
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function renderActions() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client }, createElement(RunActionsProvider, { pipeline, children }))

  return renderHook(() => useRunActions(), { wrapper })
}

for (const mode of ['all', 'failed'] as const) {
  test(`a rejected ${mode} rerun rejects the confirmation so the dialog stays open`, async () => {
    const { result } = renderActions()

    act(() => result.current.requestRerun(run))
    await assert.rejects(result.current.confirmRerun(mode), /provider refused/)

    backend.failing = false
    await act(async () => result.current.confirmRerun(mode))
    assert.equal(backend.calls.filter((command) => command.startsWith('rerun')).length, 2)
  })
}
