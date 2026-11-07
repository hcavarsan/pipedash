import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'
import { pipelineFixture } from '../../../testing/fixtures'
import type { PipelineRun, TriggerParams } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const calls: string[] = []
let failing = true
Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string) => {
      calls.push(command)

      if (failing) {
        throw new Error('provider refused')
      }
    },
  },
})

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { notifications } = await import('@mantine/notifications')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { useRunDetailActions } = await import('./useRunDetailActions')

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

afterEach(async () => {
  await act(async () => cleanup())
  notifications.clean()
  mock.restoreAll()
  calls.length = 0
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a failed rerun rejects so the confirmation dialog can stay open, and only a success closes the modal', async () => {
  mock.method(apiService, 'rerunFailedJobs', tauriService.rerunFailedJobs)
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
  const closed: number[] = []
  const { result } = renderHook(
    () =>
      useRunDetailActions({
        pipelineId: 'p1',
        runNumber: 7,
        pipeline: undefined,
        run,
        rerunInPlace: false,
        onClose: () => closed.push(1),
        onCancelRequested: () => undefined,
      }),
    { wrapper }
  )

  await assert.rejects(result.current.rerun('failed'), /provider refused/)
  assert.equal(closed.length, 0)

  failing = false
  await act(async () => result.current.rerun('failed'))
  assert.equal(closed.length, 1)
  assert.deepEqual(calls, ['rerun_failed_jobs', 'rerun_failed_jobs'])
  client.clear()
})

for (const scenario of [
  { name: 'missing inputs', inputs: undefined, branch: 'release/1.0', expected: { ref: 'release/1.0' } },
  {
    name: 'unrelated inputs',
    inputs: { variables: { DEPLOY: 'yes' } },
    branch: 'release/1.0',
    expected: { variables: { DEPLOY: 'yes' }, ref: 'release/1.0' },
  },
  {
    name: 'an explicit ref',
    inputs: { ref: 'release/2.0' },
    branch: 'release/1.0',
    expected: { ref: 'release/2.0' },
  },
  {
    name: 'an explicit branch',
    inputs: { branch: 'release/2.0' },
    branch: 'release/1.0',
    expected: { branch: 'release/2.0' },
  },
  { name: 'no known branch', inputs: undefined, branch: null, expected: {} },
  ...[
    { providerType: 'gitea', expected: { ref: 'release/1.0' } },
    { providerType: 'bitbucket', expected: { ref: 'release/1.0' } },
    { providerType: 'buildkite', expected: { branch: 'release/1.0' } },
    { providerType: 'azure-devops', expected: { 'source-branch': 'release/1.0' } },
    { providerType: 'woodpecker', expected: { branch: 'release/1.0' } },
    { providerType: 'jenkins', expected: {} },
    { providerType: 'unknown', expected: {} },
  ].map((scenario) => ({ ...scenario, name: scenario.providerType, inputs: undefined, branch: 'release/1.0' })),
  {
    name: 'an explicit Azure branch',
    providerType: 'azure-devops',
    inputs: { 'source-branch': 'release/2.0' },
    branch: 'release/1.0',
    expected: { 'source-branch': 'release/2.0' },
  },
]) {
  test(`rerunning with ${scenario.name} preserves the intended branch`, async () => {
    const requests: TriggerParams[] = []

    mock.method(apiService, 'triggerPipeline', async (params: TriggerParams) => {
      requests.push(params)

      return '{}'
    })
    mock.method(tauriService, 'triggerPipeline', apiService.triggerPipeline)
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)
    const inputs = scenario.inputs ? Object.freeze(scenario.inputs) : undefined
    const { result } = renderHook(
      () =>
        useRunDetailActions({
          pipelineId: run.pipeline_id,
          runNumber: run.run_number,
          pipeline: pipelineFixture({
            provider_type: 'providerType' in scenario ? scenario.providerType : 'gitlab',
          }),
          run: { ...run, branch: scenario.branch, ...(inputs ? { inputs } : {}) },
          rerunInPlace: false,
          onClose: () => undefined,
          onCancelRequested: () => undefined,
        }),
      { wrapper }
    )

    try {
      await act(async () => result.current.rerun('all'))
      assert.deepEqual(
        requests.map((request) => request.inputs ?? {}),
        [scenario.expected]
      )
    } finally {
      client.clear()
    }
  })
}
