import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { UserEvent } from '@testing-library/user-event'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { ConnectServerForm } = await import('./ConnectServerForm')
const { connectionService } = await import('../../../services/connection')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const pairs: { address: string; code: string | null; allowPrivateHttp: boolean }[] = []

beforeEach(() => {
  pairs.length = 0
  notifyManager.setNotifyFunction((callback) => act(callback))
  mock.method(connectionService, 'pair', async (address: string, code: string | null, allowPrivateHttp: boolean) => {
    pairs.push({ address, code, allowPrivateHttp })

    throw Object.assign(new Error('This server does not use encryption'), { code: 'PRIVATE_HTTP_DISABLED' })
  })
})

afterEach(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  mock.restoreAll()
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const ui = () => (
  <MantineProvider>
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConnectServerForm mobileShell={false} link="" alerts={false} />
      </MemoryRouter>
    </QueryClientProvider>
  </MantineProvider>
)

const addressField = () => view().findByLabelText(/^Server address/)
const codeField = () => view().findByLabelText(/^Pairing code/)
const connectButton = () => view().getByRole('button', { name: 'Connect' })
const consentBox = () => view().queryByRole('checkbox', { name: /Allow unencrypted connections/ })

async function requestConsent(user: UserEvent): Promise<void> {
  await user.type(await addressField(), 'http://192.168.1.5:8080')
  await user.type(await codeField(), 'ABCD-EFGH')
  await user.click(connectButton())
  await view().findByRole('checkbox', { name: /Allow unencrypted connections/ })
}

test('submitting without an address focuses the address field', async () => {
  const user = userEvent.setup({ document })
  render(ui())
  const address = await addressField()
  await user.click(connectButton())

  assert.ok(await view().findByText('Enter the server address or pairing link.'))
  await waitFor(() => assert.equal(document.activeElement, address))
  assert.equal(pairs.length, 0)
})

test('a valid address with a missing code focuses the code field', async () => {
  const user = userEvent.setup({ document })
  render(ui())
  await user.type(await addressField(), 'https://pipedash.example.com')
  await user.click(connectButton())

  assert.ok(await view().findByText('Enter the 8-character code.'))
  await waitFor(async () => assert.equal(document.activeElement, await codeField()))
  assert.equal(pairs.length, 0)
})

test('the encryption prompt disappears when the address changes', async () => {
  const user = userEvent.setup({ document })
  render(ui())
  await requestConsent(user)

  await user.type(await codeField(), 'X')
  assert.ok(consentBox())

  await user.type(await addressField(), '0')
  await waitFor(() => assert.equal(consentBox(), null))
})

test('consent given for one address is not sent for another', async () => {
  const user = userEvent.setup({ document })
  render(ui())
  await requestConsent(user)

  await user.click(await view().findByRole('checkbox', { name: /Allow unencrypted connections/ }))
  await user.type(await addressField(), '0')
  await user.click(connectButton())

  await waitFor(() => assert.equal(pairs.length, 2))
  assert.deepEqual(pairs[1], { address: 'http://192.168.1.5:80800', code: 'ABCD-EFGH', allowPrivateHttp: false })
})
