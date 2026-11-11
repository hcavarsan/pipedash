import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 390, height: 844 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { mockIPC } = await import('@tauri-apps/api/mocks')
const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')

const calls: Array<{ command: string; args: unknown }> = []
let failure: Error | null = null
let rejectedPipeline: string | null = null
mockIPC((command, args) => {
  calls.push({ command, args })
  if (failure) {
    throw failure
  }
  if (isRecord(args) && args.pipelineId === rejectedPipeline) {
    throw new Error('The provider refused the change')
  }
  return { imported: 2, skipped: 0 }
})
const { useLegacyPipelineMarksImport, useSetPipelineMarks } = await import('./usePipelineViewQueries')
const { logger } = await import('../../../lib/logger')
const { notifications } = await import('../../../lib/notifications')
const { queryKeys } = await import('../../../lib/queryKeys')

const client = new QueryClient()
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

beforeEach(() => {
  calls.length = 0
  failure = null
  rejectedPipeline = null
  localStorage.setItem('pipedash-pinned-pipelines', JSON.stringify({ state: { pinnedKeys: ['GitHub:github__a', 3] } }))
  localStorage.setItem('pipedash-hidden-pipelines', JSON.stringify({ state: { hiddenKeys: ['GitHub:github__b'] } }))
})
afterEach(() => {
  cleanup()
  client.clear()
})
after(() => GlobalRegistrator.unregister())

test('imports both legacy keys and removes them on success', async () => {
  renderHook(() => useLegacyPipelineMarksImport(true), { wrapper })

  await waitFor(() => assert.equal(localStorage.getItem('pipedash-pinned-pipelines'), null))
  assert.equal(localStorage.getItem('pipedash-hidden-pipelines'), null)
  assert.deepEqual(calls, [
    {
      command: 'import_legacy_pipeline_marks',
      args: { marks: { pinned: ['GitHub:github__a'], hidden: ['GitHub:github__b'] } },
    },
  ])
})

test('keeps both legacy keys when the import fails', async (context) => {
  const warn = context.mock.method(logger, 'warn', () => undefined)
  failure = new Error('database is locked')
  renderHook(() => useLegacyPipelineMarksImport(true), { wrapper })

  await waitFor(() => assert.equal(warn.mock.callCount(), 1))
  assert.notEqual(localStorage.getItem('pipedash-pinned-pipelines'), null)
  assert.notEqual(localStorage.getItem('pipedash-hidden-pipelines'), null)
})

test('does not call the backend while disabled and imports once when enabled later', async () => {
  const { rerender } = renderHook(({ enabled }) => useLegacyPipelineMarksImport(enabled), {
    wrapper,
    initialProps: { enabled: false },
  })

  assert.equal(calls.length, 0)
  assert.notEqual(localStorage.getItem('pipedash-pinned-pipelines'), null)

  rerender({ enabled: true })
  await waitFor(() => assert.equal(calls.length, 1))
  rerender({ enabled: false })
  rerender({ enabled: true })

  assert.equal(calls.length, 1)
})

test('missing, empty or malformed legacy data makes no request and is cleaned up', async () => {
  localStorage.setItem('pipedash-pinned-pipelines', '{not json')
  localStorage.setItem('pipedash-hidden-pipelines', JSON.stringify({ state: { hiddenKeys: [] } }))
  renderHook(() => useLegacyPipelineMarksImport(true), { wrapper })

  await waitFor(() => assert.equal(localStorage.getItem('pipedash-pinned-pipelines'), null))
  assert.equal(localStorage.getItem('pipedash-hidden-pipelines'), null)
  assert.equal(calls.length, 0)
})

test('rerendering after a successful import does not import again', async () => {
  const { rerender } = renderHook(() => useLegacyPipelineMarksImport(true), { wrapper })
  await waitFor(() => assert.equal(localStorage.getItem('pipedash-pinned-pipelines'), null))

  rerender()

  assert.equal(calls.length, 1)
})

const pageInvalidations = (invalidations: { mock: { calls: Array<{ arguments: unknown[] }> } }): number =>
  invalidations.mock.calls.filter((call) => {
    const filters = call.arguments[0]

    return isRecord(filters) && JSON.stringify(filters.queryKey) === JSON.stringify(queryKeys.pipelines.pages())
  }).length

test('a bulk pin attempts every pipeline, toasts the first failure once and reconciles once', async (context) => {
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  const invalidations = context.mock.method(client, 'invalidateQueries')
  rejectedPipeline = 'b'
  const { result } = renderHook(() => useSetPipelineMarks(), { wrapper })

  await act(async () => {
    result.current.mutate({ mark: 'pinned', pipelineIds: ['a', 'b', 'c'], value: true })
  })
  await waitFor(() => assert.equal(result.current.isError, true))

  assert.deepEqual(
    calls.map((call) => [call.command, isRecord(call.args) ? call.args.pipelineId : null]),
    [
      ['set_pipeline_view_membership', 'a'],
      ['set_pipeline_view_membership', 'b'],
      ['set_pipeline_view_membership', 'c'],
    ]
  )
  assert.equal(shown.mock.callCount(), 1)
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, "Couldn't update pin")
  assert.match(String(shown.mock.calls[0]?.arguments[0]?.message), /The provider refused the change/)
  assert.equal(pageInvalidations(invalidations), 1)
})

test('hiding in bulk uses the hidden command with its own error title', async (context) => {
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  rejectedPipeline = 'a'
  const { result } = renderHook(() => useSetPipelineMarks(), { wrapper })

  await act(async () => {
    result.current.mutate({ mark: 'hidden', pipelineIds: ['a', 'b'], value: true })
  })
  await waitFor(() => assert.equal(result.current.isError, true))

  assert.deepEqual(
    calls.map((call) => call.command),
    ['set_pipeline_hidden', 'set_pipeline_hidden']
  )
  assert.equal(shown.mock.calls[0]?.arguments[0]?.title, "Couldn't update hidden pipelines")
})
