import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineFixture, providerSummaryFixture } from '../../../testing/fixtures'
import type { LocalNotificationSettings } from '../../../types/notifications'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' },
  __TAURI_INTERNALS__: { invoke: async () => null },
})

const { cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { queryKeys } = await import('../../../lib/queryKeys')
const { mobileService } = await import('../../../services/mobile')
const { NotificationsSection } = await import('./NotificationsSection')

const view = () => within(document.body)
const settings: LocalNotificationSettings = {
  enabled: false,
  available: true,
  reason: null,
  alertHost: null,
  pushProblem: null,
}
let reads = 0

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  client.setQueryData(queryKeys.pipelines.list(), [pipelineFixture({ id: 'stub__1__acme/api__ci.yml' })])
  client.setQueryData(queryKeys.providers.list(), [providerSummaryFixture({ id: 1, name: 'Stub' })])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <NotificationsSection />
      </QueryClientProvider>
    </MantineProvider>
  )
}

const isDisabled = (label: string): boolean => {
  const control = view().getByRole('switch', { name: new RegExp(`^${label}`) })

  return control instanceof HTMLInputElement && control.disabled
}

beforeEach(() => {
  reads = 0
  mock.method(mobileService, 'getLocalNotificationSettings', async () => {
    reads += 1

    if (reads === 1) {
      throw new Error('Host unreachable')
    }

    return settings
  })
})

afterEach(() => {
  cleanup()
  mock.restoreAll()
})

after(() => GlobalRegistrator.unregister())

test('a failed device settings read shows Retry, keeps other preferences usable and a retry enables the switch', async () => {
  const user = userEvent.setup({ document })
  mount()

  await view().findByText("Couldn't read the notification settings")
  assert.ok(view().getByText('Host unreachable'))
  assert.equal(isDisabled('Enable notifications'), true)
  assert.equal(isDisabled('Failed runs'), false)

  await user.click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.equal(isDisabled('Enable notifications'), false))
  assert.equal(view().queryByText("Couldn't read the notification settings"), null)
  assert.equal(view().queryByRole('button', { name: 'Retry' }), null)
  assert.equal(reads, 2)
})
