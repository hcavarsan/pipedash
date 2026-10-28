import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { RenderResult } from '@testing-library/react'
import { createElement, useState } from 'react'

import type { SignInStart, SignInStatus } from '../../../types'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services')
const { SignInField } = await import('./SignInField')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const view = () => within(document.body)

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  GlobalRegistrator.unregister()
})

const started = (id: string): SignInStart => ({
  id,
  authorize_url: 'https://argo.example.com/api/dex/auth',
  label: 'GitHub',
  manual: false,
})

function Form({ onChange }: { onChange: (reference: string | null) => void }) {
  const [reference, setReference] = useState<string | null>(null)

  return createElement(SignInField, {
    id: 'sign-in',
    providerType: 'argocd',
    configValues: reference
      ? { server_url: 'https://argo.example.com', sign_in: reference }
      : { server_url: 'https://argo.example.com' },
    ready: true,
    disabled: false,
    shell: false,
    onChange: (next) => {
      onChange(next)
      setReference(next)
    },
  })
}

const renderField = async (onChange: (reference: string | null) => void): Promise<RenderResult> => {
  let rendered: RenderResult | undefined

  await act(async () => {
    rendered = render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(Form, { onChange }))
      )
    )
  })

  return rendered as RenderResult
}

const neverSettles = () => new Promise<SignInStatus>(() => undefined)

test('closing the form while the sign-in is starting cancels the new flow', async (context) => {
  const start = Promise.withResolvers<SignInStart>()
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  const changes: (string | null)[] = []
  context.mock.method(apiService, 'startSignIn', () => start.promise)
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const field = await renderField((reference) => changes.push(reference))
  fireEvent.click(view().getByRole('button', { name: 'Sign in with SSO' }))
  await act(async () => field.unmount())
  await act(async () => start.resolve(started('late-flow')))

  await waitFor(() =>
    assert.deepEqual(
      cancelled.mock.calls.map((call) => call.arguments[0]),
      ['late-flow']
    )
  )
  assert.deepEqual(changes, [])
})

test('closing the form before the first status arrives cancels the started flow', async (context) => {
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  const changes: (string | null)[] = []
  context.mock.method(apiService, 'startSignIn', async () => started('fresh-flow'))
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const field = await renderField((reference) => changes.push(reference))
  fireEvent.click(view().getByRole('button', { name: 'Sign in with SSO' }))
  await waitFor(() => assert.deepEqual(changes, ['fresh-flow']))
  await act(async () => field.unmount())

  await waitFor(() =>
    assert.deepEqual(
      cancelled.mock.calls.map((call) => call.arguments[0]),
      ['fresh-flow']
    )
  )
})

test('a failed status check keeps the started sign-in until the status loads', async (context) => {
  let failing = true
  context.mock.method(apiService, 'startSignIn', async () => started('flaky-flow'))
  context.mock.method(apiService, 'signInStatus', async (): Promise<SignInStatus> => {
    if (failing) {
      throw new Error('Server restarting')
    }
    return { state: 'completed', account: 'dev@example.com' }
  })
  context.mock.method(apiService, 'cancelSignIn', async () => undefined)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const field = await renderField(() => undefined)
  fireEvent.click(view().getByRole('button', { name: 'Sign in with SSO' }))

  assert.ok(await view().findByText(/Server restarting/))
  assert.ok(view().getByRole('button', { name: 'Cancel' }))
  assert.equal(view().queryByRole('button', { name: 'Sign in with SSO' }), null)

  failing = false
  await act(async () => {
    await client.refetchQueries({ queryKey: ['signIn'] })
  })
  assert.ok(await view().findByText('Signed in as dev@example.com'))
  assert.equal(view().queryByText(/Server restarting/), null)
  await act(async () => field.unmount())
})
