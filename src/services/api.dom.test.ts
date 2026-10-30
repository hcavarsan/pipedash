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

test('provider recovery opens a tab synchronously before starting the provider-bound flow', async (context) => {
  const events: string[] = []
  const start = {
    id: 'provider-flow',
    authorize_url: 'https://argo.example.com/api/dex/auth',
    label: 'GitHub',
    manual: false,
  }
  context.mock.method(window, 'open', () => {
    events.push('open')

    return null
  })
  context.mock.method(globalThis, 'fetch', async (url: string, init: RequestInit) => {
    events.push(`${init.method} ${url}`)
    assert.equal(init.body, undefined)

    return new Response(JSON.stringify(start), { status: 200 })
  })

  const pending = apiService.startProviderSignIn(7)

  assert.equal(events[0], 'open')
  assert.deepEqual(await pending, start)
  assert.deepEqual(events, ['open', 'POST /api/v1/providers/7/sign-in'])
})

test('reconnecting sends only the completed flow and never automatically retries a failed commit', async (context) => {
  const sent: { url: string; method: string | undefined; body: unknown }[] = []
  context.mock.method(globalThis, 'fetch', async (url: string, init: RequestInit) => {
    sent.push({ url, method: init.method, body: init.body })

    return providerError()
  })

  await assert.rejects(apiService.reconnectProvider(7, 'completed-flow'))
  assert.deepEqual(sent, [
    {
      url: '/api/v1/providers/7/sign-in/complete',
      method: 'POST',
      body: JSON.stringify({ flow_id: 'completed-flow' }),
    },
  ])
})
