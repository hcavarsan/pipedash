import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'
import { pluginPresentationFixture } from '../../../testing/presentation'
import type { TriggerDraft } from '../../../types/pipelines'

GlobalRegistrator.register()

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { useTriggerValidation } = await import('./useTriggerValidation')

const draft = (errors: Record<string, string>): TriggerDraft => ({
  parameters: [],
  values: {},
  fields: [],
  errors,
  availability: { allowed: true, reason: null },
  copy: pluginPresentationFixture().trigger,
})

afterEach(async () => {
  await act(async () => cleanup())
  mock.restoreAll()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('typing in another field does not drop a pending blur error', async () => {
  const gates: PromiseWithResolvers<void>[] = []
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })

  mock.method(apiService, 'preparePipelineTrigger', async () => {
    const gate = Promise.withResolvers<void>()

    gates.push(gate)
    await gate.promise

    return draft({ ref: 'Enter a branch' })
  })

  const { result } = renderHook(() => useTriggerValidation('p1', undefined), {
    wrapper: ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children),
  })

  act(() => result.current.blur('ref', { ref: '', env: '' }))
  act(() => result.current.change('env', { ref: '', env: 'prod' }))
  await waitFor(() => assert.ok(gates.length >= 1))
  await act(async () => {
    for (const gate of gates) {
      gate.resolve()
    }
  })

  await waitFor(() => assert.equal(result.current.errors.ref, 'Enter a branch'))
  assert.equal(result.current.errors.env, undefined)
})
