import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

if (process.env.PIPEDASH_LEGACY_MARKS_TEST !== '1') {
  test('legacy pin and hide marks import in an isolated runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_LEGACY_MARKS_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 390, height: 844 })
  const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { mockIPC } = await import('@tauri-apps/api/mocks')
  const { cleanup, renderHook, waitFor } = await import('@testing-library/react')

  const calls: Array<{ command: string; args: unknown }> = []
  let failure: Error | null = null
  mockIPC((command, args) => {
    calls.push({ command, args })
    if (failure) {
      throw failure
    }
    return { imported: 2, skipped: 0 }
  })
  const { useLegacyPipelineMarksImport } = await import('./usePipelineViewQueries')
  const { logger } = await import('../lib/logger')

  const client = new QueryClient()
  const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

  beforeEach(() => {
    calls.length = 0
    failure = null
    localStorage.setItem(
      'pipedash-pinned-pipelines',
      JSON.stringify({ state: { pinnedKeys: ['GitHub:github__a', 3] } })
    )
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
}
