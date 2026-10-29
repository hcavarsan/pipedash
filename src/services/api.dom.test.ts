import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ url: 'http://127.0.0.1:8080/', width: 1400, height: 900 })

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { notifyManager, QueryClientProvider } = await import('@tanstack/react-query')
const { queryClient } = await import('../lib/queryClient')
const { useTriggerWorkflow } = await import('../features/runs/queries/useWorkflowQueries')
const { apiService } = await import('./api')

notifyManager.setNotifyFunction((callback) => act(callback))

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(QueryClientProvider, { client: queryClient }, children)

const providerError = () =>
  new Response(JSON.stringify({ code: 'PROVIDER_ERROR', message: 'API error: HTTP 500' }), { status: 500 })

afterEach(async () => {
  await act(async () => cleanup())
  queryClient.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('one failed trigger click sends the trigger request once', async (context) => {
  const sent: string[] = []
  context.mock.method(globalThis, 'fetch', async (url: string, init: RequestInit) => {
    sent.push(`${init.method} ${url}`)

    return providerError()
  })
  const { result } = renderHook(() => useTriggerWorkflow(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync({ workflow_id: 'jenkins__1__deploy', inputs: {} }).catch(() => undefined)
  })

  assert.deepEqual(sent, ['POST /api/v1/pipelines/jenkins__1__deploy/trigger'])
})

test('a read that hits a 5xx is sent again and returns the next answer', async (context) => {
  let sent = 0
  context.mock.method(globalThis, 'fetch', async () => {
    sent += 1

    return sent === 1 ? providerError() : new Response(JSON.stringify([]), { status: 200 })
  })

  assert.deepEqual(await apiService.listProviders(), [])
  assert.equal(sent, 2)
})
