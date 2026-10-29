import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { PushStatus } from '../../types'

GlobalRegistrator.register({ width: 1280, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { PushNotificationsCard } = await import('./PushNotificationsCard')
const { mobileService } = await import('../../services/mobile')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

const unpaired: PushStatus = { supported: true, relay_configured: true, permission: 'prompt', servers: [] }

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const renderCard = async (): Promise<void> => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(PushNotificationsCard))
      )
    )
  })
}

const fields = async () => {
  const url = await view().findByRole('textbox', { name: /server url/i })
  const token = view().getByLabelText(/api token/i)
  const label = view().getByRole('textbox', { name: /label/i })
  assert.ok(url instanceof HTMLInputElement)
  assert.ok(token instanceof HTMLInputElement)
  assert.ok(label instanceof HTMLInputElement)

  return { url, token, label }
}

test('pairing rejects an https:// server URL without calling the pair service', async (context) => {
  const user = userEvent.setup({ document })
  const pair = context.mock.method(mobileService, 'pushPairServer', async () => unpaired)
  context.mock.method(mobileService, 'getPushStatus', async () => unpaired)
  await renderCard()
  const { url, token } = await fields()

  await user.type(url, 'https://')
  await user.type(token, 'secret')
  await user.click(view().getByRole('button', { name: 'Pair server' }))
  await waitFor(() => assert.equal(url.getAttribute('aria-invalid'), 'true'))
  const describedBy = url.getAttribute('aria-describedby') ?? ''
  assert.ok(describedBy.split(' ').some((id) => document.getElementById(id)?.textContent?.match(/http/i)))
  await waitFor(() => assert.equal(url.getAttribute('aria-invalid'), 'true'))
  assert.equal(pair.mock.callCount(), 0)
  await act(async () => cleanup())
  client.clear()
})

test('pairing disables inputs while pending, keeps values on failure and clears them on success', async (context) => {
  const user = userEvent.setup({ document })
  const attempts: PromiseWithResolvers<PushStatus>[] = []
  const pair = context.mock.method(mobileService, 'pushPairServer', () => {
    const attempt = Promise.withResolvers<PushStatus>()
    attempts.push(attempt)

    return attempt.promise
  })
  context.mock.method(mobileService, 'getPushStatus', async () => unpaired)
  await renderCard()
  const { url, token, label } = await fields()

  await user.type(url, 'http://relay.local')
  await user.type(token, 'secret')
  await user.type(label, 'Work')
  await user.click(view().getByRole('button', { name: 'Pair server' }))

  await waitFor(() => {
    assert.equal(pair.mock.callCount(), 1)
    assert.equal(url.disabled, true)
    assert.equal(token.disabled, true)
    assert.equal(label.disabled, true)
  })

  await act(async () => attempts[0]?.reject(new Error('Relay refused')))
  await waitFor(() => assert.equal(url.disabled, false))
  assert.equal(url.value, 'http://relay.local')
  assert.equal(token.value, 'secret')
  assert.equal(label.value, 'Work')

  await user.click(view().getByRole('button', { name: 'Pair server' }))
  await waitFor(() => assert.equal(pair.mock.callCount(), 2))
  await act(async () => attempts[1]?.resolve(unpaired))
  await waitFor(() => {
    assert.equal(url.value, '')
    assert.equal(token.value, '')
    assert.equal(label.value, '')
  })
})
