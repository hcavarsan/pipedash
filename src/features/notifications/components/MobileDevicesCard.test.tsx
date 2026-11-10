import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { MobileDevice } from '../../../types/notifications'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('../../../lib/notifications')
const { apiService } = await import('../../../services/api')
const { MobileDevicesCard } = await import('./MobileDevicesCard')

const device: MobileDevice = {
  id: 'sub-1',
  device_name: "Henrique's iPhone",
  platform: 'ios',
  created_at: '2026-11-01T00:00:00Z',
  last_success_at: null,
}
const view = () => within(document.body)
let devices: MobileDevice[]

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <ModalsProvider>
          <MobileDevicesCard />
        </ModalsProvider>
      </QueryClientProvider>
    </MantineProvider>
  )
}

beforeEach(() => {
  devices = [device]
  mock.method(apiService, 'listMobileDevices', async () => ({ relay_configured: true, subscriptions: devices }))
})

afterEach(() => {
  cleanup()
  mock.restoreAll()
})

after(() => GlobalRegistrator.unregister())

test('removing a push device sends nothing until it is confirmed, and cancel sends nothing', async () => {
  const remove = mock.method(apiService, 'removeMobileDevice', async () => undefined)
  const user = userEvent.setup({ document })
  mount()

  await user.click(await view().findByRole('button', { name: "Remove Henrique's iPhone" }))
  const confirmation = await view().findByRole('dialog', { name: "Remove Henrique's iPhone?" })
  assert.match(confirmation.textContent ?? '', /It stops receiving push alerts from this host\./)
  assert.equal(remove.mock.callCount(), 0)

  await user.click(within(confirmation).getByRole('button', { name: 'Cancel' }))
  await waitFor(() => assert.equal(view().queryByRole('dialog'), null))

  assert.equal(remove.mock.callCount(), 0)
  assert.ok(view().getByRole('button', { name: "Remove Henrique's iPhone" }))
})

test('confirming sends one request and shows that the device no longer receives push alerts', async () => {
  const remove = mock.method(apiService, 'removeMobileDevice', async () => {
    devices = []
  })
  const shown = mock.method(notifications, 'show')
  const user = userEvent.setup({ document })
  mount()

  await user.click(await view().findByRole('button', { name: "Remove Henrique's iPhone" }))
  const confirmation = await view().findByRole('dialog', { name: "Remove Henrique's iPhone?" })
  await user.click(within(confirmation).getByRole('button', { name: 'Remove' }))

  await waitFor(() => assert.equal(remove.mock.callCount(), 1))
  assert.deepEqual(remove.mock.calls[0]?.arguments, ['sub-1'])
  await waitFor(() =>
    assert.ok(
      shown.mock.calls.some(
        (call) =>
          call.arguments[0]?.title === 'Device removed' &&
          call.arguments[0]?.message === "Henrique's iPhone no longer receives push alerts."
      )
    )
  )
  await waitFor(() => assert.equal(view().queryByRole('button', { name: "Remove Henrique's iPhone" }), null))
  assert.equal(remove.mock.callCount(), 1)
})
