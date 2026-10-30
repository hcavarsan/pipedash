import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment } from 'react'

import type { SignInStart, SignInStatus } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

// DOM globals must exist before React DOM and Mantine initialize.
const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { useProviderStore } = await import('../../../stores/providerStore')
const { queryKeys } = await import('../../../lib/queryKeys')
const { useProviderSignIn } = await import('../queries/useProviderSignIn')
const { ProviderSignInModal } = await import('./ProviderSignInModal')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const view = () => within(document.body)
const started = (id: string, manual = false): SignInStart => ({
  id,
  authorize_url: 'https://argo.example.com/api/dex/auth',
  label: 'GitHub',
  manual,
})
const neverSettles = () => new Promise<SignInStatus>(() => undefined)

notifyManager.setNotifyFunction((callback) => act(callback))

afterEach(async () => {
  await act(async () => cleanup())
  useProviderStore.setState({ providerSignIn: null })
  client.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

function Harness() {
  const signIn = useProviderSignIn()

  return createElement(
    Fragment,
    null,
    createElement('button', { type: 'button', onClick: () => signIn(7) }, 'Reconnect first'),
    createElement('button', { type: 'button', onClick: () => signIn(9) }, 'Reconnect second'),
    createElement(ProviderSignInModal)
  )
}

const renderRecovery = () =>
  act(async () =>
    render(createElement(MantineProvider, null, createElement(QueryClientProvider, { client }, createElement(Harness))))
  )

test('completed sign-in automatically reconnects once and invalidates provider caches', async (context) => {
  const commit = Promise.withResolvers<void>()
  const reconnect = context.mock.fn(() => commit.promise)
  context.mock.method(apiService, 'startProviderSignIn', async () => started('completed-flow'))
  context.mock.method(
    apiService,
    'signInStatus',
    async (): Promise<SignInStatus> => ({ state: 'completed', account: null })
  )
  context.mock.method(apiService, 'reconnectProvider', reconnect)
  context.mock.method(apiService, 'cancelSignIn', async () => undefined)
  client.setQueryData(queryKeys.providers.detail(7), { name: 'Provider' })

  await renderRecovery()
  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Reconnect first' }))
  await waitFor(() => assert.equal(reconnect.mock.callCount(), 1))
  assert.deepEqual(reconnect.mock.calls[0]?.arguments, [7, 'completed-flow'])
  assert.ok(view().getByText('Saving your sign-in and reconnecting the provider…'))
  await act(async () => {
    client.setQueryData(queryKeys.signIn.status('completed-flow'), { state: 'completed', account: null })
  })
  assert.equal(reconnect.mock.callCount(), 1)

  await act(async () => commit.resolve())
  await waitFor(() => assert.equal(useProviderStore.getState().providerSignIn, null))
  assert.equal(client.getQueryState(queryKeys.providers.detail(7))?.isInvalidated, true)
  assert.equal(view().queryByRole('dialog'), null)
})

test('the first tap starts synchronously and repeated taps cannot change the active provider', async (context) => {
  const start = Promise.withResolvers<SignInStart>()
  const begin = context.mock.fn(() => start.promise)
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', begin)
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  await renderRecovery()
  const firstButton = view().getByRole('button', { name: 'Reconnect first' })
  const secondButton = view().getByRole('button', { name: 'Reconnect second' })

  act(() => {
    fireEvent.click(firstButton)
    assert.equal(begin.mock.callCount(), 1)
    fireEvent.click(firstButton)
    fireEvent.click(secondButton)
    assert.equal(begin.mock.callCount(), 1)
    assert.equal(useProviderStore.getState().providerSignIn?.providerId, 7)
  })

  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Cancel' }))
  await act(async () => start.resolve(started('late-flow')))
  await waitFor(() => assert.equal(cancelled.mock.callCount(), 1))
  assert.deepEqual(cancelled.mock.calls[0]?.arguments, ['late-flow'])
  assert.equal(useProviderStore.getState().providerSignIn, null)
  assert.equal(view().queryByRole('dialog'), null)
})

test('dismissing a waiting flow cancels it and does not reconnect after its status completes', async (context) => {
  const status = Promise.withResolvers<SignInStatus>()
  const reconnect = context.mock.fn(async (_id: number, _flowId: string) => undefined)
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', async () => started('cancelled-flow'))
  context.mock.method(apiService, 'signInStatus', () => status.promise)
  context.mock.method(apiService, 'reconnectProvider', reconnect)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  await renderRecovery()
  const user = userEvent.setup({ document })

  await user.click(view().getByRole('button', { name: 'Reconnect first' }))
  await view().findByText('Waiting for you to sign in with GitHub…')
  await user.click(view().getByRole('button', { name: 'Cancel' }))
  await act(async () => status.resolve({ state: 'completed', account: null }))

  assert.equal(cancelled.mock.callCount(), 1)
  assert.deepEqual(cancelled.mock.calls[0]?.arguments, ['cancelled-flow'])
  assert.equal(reconnect.mock.callCount(), 0)
  assert.equal(view().queryByRole('dialog'), null)
})

test('failed commit stays retryable without a second login or token exchange', async (context) => {
  const retry = Promise.withResolvers<void>()
  const begin = context.mock.fn(async () => started('retry-flow'))
  const complete = context.mock.fn(async (): Promise<SignInStatus> => ({ state: 'completed', account: null }))
  let attempts = 0
  const reconnect = context.mock.fn(() => {
    attempts += 1

    return attempts === 1 ? Promise.reject(new Error('Vault is locked')) : retry.promise
  })
  context.mock.method(apiService, 'startProviderSignIn', begin)
  context.mock.method(
    apiService,
    'signInStatus',
    async (): Promise<SignInStatus> => ({ state: 'completed', account: null })
  )
  context.mock.method(apiService, 'completeSignIn', complete)
  context.mock.method(apiService, 'reconnectProvider', reconnect)
  context.mock.method(apiService, 'cancelSignIn', async () => undefined)
  await renderRecovery()
  const user = userEvent.setup({ document })

  await user.click(view().getByRole('button', { name: 'Reconnect first' }))
  assert.ok(await view().findByText(/Vault is locked/))
  assert.equal(reconnect.mock.callCount(), 1)
  await act(async () => {
    client.setQueryData(queryKeys.signIn.status('retry-flow'), { state: 'completed', account: null })
  })
  assert.equal(reconnect.mock.callCount(), 1)
  await user.click(view().getByRole('button', { name: 'Retry saving' }))
  await waitFor(() => assert.equal(reconnect.mock.callCount(), 2))
  assert.equal(begin.mock.callCount(), 1)
  assert.equal(complete.mock.callCount(), 0)
  assert.equal(view().getByRole('button', { name: 'Cancel' }).hasAttribute('disabled'), true)

  await act(async () => retry.resolve())
  await waitFor(() => assert.equal(useProviderStore.getState().providerSignIn, null))
})

test('manual redirect completion saves the existing recovery flow automatically', async (context) => {
  const finish = context.mock.fn(
    async (): Promise<SignInStatus> => ({ state: 'completed', account: 'dev@example.com' })
  )
  const reconnect = context.mock.fn(async (_id: number, _flowId: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', async () => started('manual-flow', true))
  context.mock.method(apiService, 'signInStatus', async (): Promise<SignInStatus> => ({ state: 'pending' }))
  context.mock.method(apiService, 'completeSignIn', finish)
  context.mock.method(apiService, 'reconnectProvider', reconnect)
  context.mock.method(apiService, 'cancelSignIn', async () => undefined)
  await renderRecovery()
  const user = userEvent.setup({ document })

  await user.click(view().getByRole('button', { name: 'Reconnect first' }))
  await user.type(
    await view().findByLabelText('Address of the page after sign-in'),
    'https://argo.example.com/callback?code=new'
  )
  await user.click(view().getByRole('button', { name: 'Continue' }))
  await waitFor(() => assert.equal(reconnect.mock.callCount(), 1))
  assert.deepEqual(finish.mock.calls[0]?.arguments, ['manual-flow', 'https://argo.example.com/callback?code=new'])
  assert.deepEqual(reconnect.mock.calls[0]?.arguments, [7, 'manual-flow'])
})

test('unmount cancels a flow that finishes starting later', async (context) => {
  const start = Promise.withResolvers<SignInStart>()
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', () => start.promise)
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  const recovery = await renderRecovery()

  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Reconnect first' }))
  await act(async () => recovery.unmount())
  await act(async () => start.resolve(started('unmounted-flow')))
  await waitFor(() => assert.equal(cancelled.mock.callCount(), 1))
  assert.deepEqual(cancelled.mock.calls[0]?.arguments, ['unmounted-flow'])
  assert.equal(useProviderStore.getState().providerSignIn, null)
})

test('failed start retries explicitly without reopening a dismissed sheet', async (context) => {
  let attempts = 0
  context.mock.method(apiService, 'startProviderSignIn', async () => {
    attempts += 1

    if (attempts === 1) {
      throw new Error('Identity provider unavailable')
    }

    return started('retried-start')
  })
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', async () => undefined)
  await renderRecovery()
  const user = userEvent.setup({ document })

  await user.click(view().getByRole('button', { name: 'Reconnect first' }))
  assert.ok(await view().findByText(/Identity provider unavailable/))
  await user.click(view().getByRole('button', { name: 'Sign in again' }))
  assert.ok(await view().findByText('Waiting for you to sign in with GitHub…'))
  assert.equal(attempts, 2)
  await user.click(view().getByRole('button', { name: 'Cancel' }))
  assert.equal(view().queryByRole('dialog'), null)
  assert.equal(attempts, 2)
})

test('a cancelled late start cannot replace or commit the next provider session', async (context) => {
  const first = Promise.withResolvers<SignInStart>()
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  const reconnect = context.mock.fn(async (_id: number, _flowId: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', (id: number) =>
    id === 7 ? first.promise : Promise.resolve(started('second-provider'))
  )
  context.mock.method(apiService, 'signInStatus', neverSettles)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  context.mock.method(apiService, 'reconnectProvider', reconnect)
  await renderRecovery()
  const user = userEvent.setup({ document })

  await user.click(view().getByRole('button', { name: 'Reconnect first' }))
  await user.click(view().getByRole('button', { name: 'Cancel' }))
  await user.click(view().getByRole('button', { name: 'Reconnect second' }))
  await view().findByText('Waiting for you to sign in with GitHub…')
  await act(async () => first.resolve(started('first-provider-late')))

  assert.deepEqual(cancelled.mock.calls[0]?.arguments, ['first-provider-late'])
  assert.equal(useProviderStore.getState().providerSignIn?.providerId, 9)
  assert.equal(useProviderStore.getState().providerSignIn?.flow?.id, 'second-provider')
  await act(async () => {
    client.setQueryData(queryKeys.signIn.status('second-provider'), { state: 'completed', account: null })
  })
  await waitFor(() => assert.equal(reconnect.mock.callCount(), 1))
  assert.deepEqual(reconnect.mock.calls[0]?.arguments, [9, 'second-provider'])
})

test('unmount during a failed commit discards the retained completed flow', async (context) => {
  const commit = Promise.withResolvers<void>()
  const cancelled = context.mock.fn(async (_id: string) => undefined)
  context.mock.method(apiService, 'startProviderSignIn', async () => started('unmounted-commit'))
  context.mock.method(
    apiService,
    'signInStatus',
    async (): Promise<SignInStatus> => ({ state: 'completed', account: null })
  )
  context.mock.method(apiService, 'reconnectProvider', () => commit.promise)
  context.mock.method(apiService, 'cancelSignIn', cancelled)
  const recovery = await renderRecovery()

  await userEvent.setup({ document }).click(view().getByRole('button', { name: 'Reconnect first' }))
  await view().findByText('Saving your sign-in and reconnecting the provider…')
  await act(async () => recovery.unmount())
  await act(async () => commit.reject(new Error('Vault locked')))

  await waitFor(() => assert.equal(cancelled.mock.callCount(), 1))
  assert.deepEqual(cancelled.mock.calls[0]?.arguments, ['unmounted-commit'])
  assert.equal(useProviderStore.getState().providerSignIn, null)
})
