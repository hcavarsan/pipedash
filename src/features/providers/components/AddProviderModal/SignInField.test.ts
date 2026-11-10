import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { RenderResult } from '@testing-library/react'
import { createElement, useId, useState } from 'react'

import type { SignInStart, SignInStatus } from '../../../../types/providers'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../../services/api')
const { SignInField } = await import('./SignInField')
const { notifications } = await import('../../../../lib/notifications')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const view = () => within(document.body)

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const started = (id: string): SignInStart => ({
  id,
  authorize_url: 'https://argo.example.com/api/dex/auth',
  label: 'GitHub',
  manual: false,
})

function Form({ onChange }: { onChange: (reference: string | null) => void }) {
  const [reference, setReference] = useState<string | null>(null)
  const id = useId()

  return createElement(SignInField, {
    id,
    providerType: 'argocd',
    configValues: reference
      ? { server_url: 'https://argo.example.com', sign_in: reference }
      : { server_url: 'https://argo.example.com' },
    missing: [],
    disabled: false,
    shell: false,
    onChange: (next) => {
      onChange(next)
      setReference(next)
    },
  })
}

const renderField = (onChange: (reference: string | null) => void): Promise<RenderResult> =>
  act(async () =>
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(Form, { onChange }))
      )
    )
  )

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
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Sign in with SSO' }))
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
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Sign in with SSO' }))
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
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Sign in with SSO' }))

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

test('a rejected cancel keeps the sign-in visible, shows the button loading and reports one error', async (context) => {
  const cancelling = Promise.withResolvers<void>()
  const changes: (string | null)[] = []
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'startSignIn', async () => started('stuck-flow'))
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', () => cancelling.promise)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const field = await renderField((reference) => changes.push(reference))
  const user = userEvent.setup({ document })
  await user.click(view().getByRole('button', { name: 'Sign in with SSO' }))
  const cancel = await view().findByRole('button', { name: 'Cancel' })
  await user.click(cancel)

  await waitFor(() => assert.equal(cancel.hasAttribute('data-loading'), true))
  assert.deepEqual(changes, ['stuck-flow'])
  assert.equal(shown.mock.callCount(), 0)

  await act(async () => cancelling.reject(new Error('Server restarting')))

  await waitFor(() => assert.equal(shown.mock.callCount(), 1))
  const options = shown.mock.calls[0]?.arguments[0]
  assert.equal(options?.title, "Couldn't cancel sign-in")
  assert.equal(options?.color, 'red')
  assert.deepEqual(changes, ['stuck-flow'])
  assert.equal(cancel.hasAttribute('data-loading'), false)
  assert.ok(view().getByRole('button', { name: 'Cancel' }))
  assert.equal(view().queryByRole('button', { name: 'Sign in with SSO' }), null)
  await act(async () => field.unmount())
  assert.equal(shown.mock.callCount(), 1)
})

test('a successful cancel resets the field only after the cancellation finishes', async (context) => {
  const cancelling = Promise.withResolvers<void>()
  const changes: (string | null)[] = []
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'startSignIn', async () => started('ending-flow'))
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', () => cancelling.promise)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const field = await renderField((reference) => changes.push(reference))
  const user = userEvent.setup({ document })
  await user.click(view().getByRole('button', { name: 'Sign in with SSO' }))
  await user.click(await view().findByRole('button', { name: 'Cancel' }))

  assert.deepEqual(changes, ['ending-flow'])
  assert.ok(view().getByRole('button', { name: 'Cancel' }))

  await act(async () => cancelling.resolve())

  await waitFor(() => assert.deepEqual(changes, ['ending-flow', null]))
  assert.ok(await view().findByRole('button', { name: 'Sign in with SSO' }))
  assert.equal(view().queryByRole('button', { name: 'Cancel' }), null)
  assert.equal(shown.mock.callCount(), 0)
  await act(async () => field.unmount())
})
