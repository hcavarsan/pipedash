import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import type { ReactNode } from 'react'

import type { UpdateStatus } from '../../../types/updater'
import { updateStatusFixture } from '../fixtures/updateStatus'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
const calls: { command: string; args: unknown }[] = []
let status: UpdateStatus = { ...updateStatusFixture }
let pending: Promise<UpdateStatus> | undefined
let failure: Error | undefined
mockIPC(
  (command, args) => {
    calls.push({ command, args })
    if (failure) {
      throw failure
    }
    if (command === 'install_update') {
      return null
    }
    return pending ?? status
  },
  { shouldMockEvents: true }
)

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { DesktopUpdates } = await import('./DesktopUpdates')
const { DesktopUpdateIndicator } = await import('../../../components/common/DesktopUpdateIndicator')
const { applyUpdateStatus } = await import('../queries/useUpdateQueries')
const { queryKeys } = await import('../../../lib/queryKeys')
const view = () => within(document.body)
let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  status = { ...updateStatusFixture }
  pending = undefined
  failure = undefined
  calls.length = 0
  notifyManager.setNotifyFunction((callback) => act(callback))
})
afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  window.innerWidth = 1400
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

function Location(): ReactNode {
  return <output aria-label="Location">{useLocation().pathname}</output>
}

function mount(children: ReactNode = <DesktopUpdates />, path = '/pipelines'): void {
  client.setQueryData(queryKeys.updates.status(), status)
  client.setQueryData(queryKeys.updates.connection(), 'connected')
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[path]}>
          <ModalsProvider>
            {children}
            <Location />
          </ModalsProvider>
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
}

function ready(): void {
  status = { ...status, phase: 'ready', available_version: '0.8.1', can_install: true }
}

test('automatic off leaves manual checks and downloads available in a narrow native window', async () => {
  window.innerWidth = 390
  status = { ...status, phase: 'available', available_version: '0.8.1', can_download: true }
  mount()
  const user = userEvent.setup({ document, delay: null })
  assert.ok(view().getByRole('switch', { checked: false }))
  await user.click(view().getByRole('button', { name: 'Check for updates' }))
  await waitFor(() => assert.equal(calls.filter((call) => call.command === 'check_for_updates').length, 1))
  await waitFor(() =>
    assert.equal(view().getByRole('button', { name: 'Download update' }).hasAttribute('disabled'), false)
  )
  await user.click(view().getByRole('button', { name: 'Download update' }))
  await waitFor(() => assert.equal(calls.filter((call) => call.command === 'download_update').length, 1))
})

test('cancelling focused installation consent does not invoke install', async () => {
  ready()
  mount()
  const user = userEvent.setup({ document, delay: null })
  const button = view().getByRole('button', { name: 'Install and restart' })
  button.focus()
  await user.keyboard('{Enter}')
  const dialog = await view().findByRole('dialog', { name: 'Install Pipedash 0.8.1?' })
  await waitFor(() => assert.ok(dialog.contains(document.activeElement)))
  await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
  assert.equal(
    calls.some((call) => call.command === 'install_update'),
    false
  )
})

test('duplicate clicks create one consent and one exact-version installation request', async () => {
  ready()
  mount()
  const user = userEvent.setup({ document, delay: null })
  await user.dblClick(view().getByRole('button', { name: 'Install and restart' }))
  const dialog = await view().findByRole('dialog', { name: 'Install Pipedash 0.8.1?' })
  await user.dblClick(within(dialog).getByRole('button', { name: 'Install and restart' }))
  await waitFor(() => assert.equal(calls.filter((call) => call.command === 'install_update').length, 1))
  assert.deepEqual(calls.find((call) => call.command === 'install_update')?.args, { version: '0.8.1' })
})

test('a newer staged version requires another consent dialog', async () => {
  ready()
  mount()
  const user = userEvent.setup({ document, delay: null })
  await user.click(view().getByRole('button', { name: 'Install and restart' }))
  const first = await view().findByRole('dialog', { name: 'Install Pipedash 0.8.1?' })
  await act(async () => applyUpdateStatus(client, { ...status, revision: 2, available_version: '0.8.2' }))
  await user.click(within(first).getByRole('button', { name: 'Install and restart' }))
  assert.equal(
    calls.some((call) => call.command === 'install_update'),
    false
  )
  await waitFor(() => assert.equal(view().queryByRole('dialog', { name: 'Install Pipedash 0.8.1?' }), null))
  await user.click(view().getByRole('button', { name: 'Install and restart' }))
  const second = await view().findByRole('dialog', { name: 'Install Pipedash 0.8.2?' })
  await user.click(within(second).getByRole('button', { name: 'Install and restart' }))
  await waitFor(() =>
    assert.deepEqual(calls.find((call) => call.command === 'install_update')?.args, { version: '0.8.2' })
  )
})

test('the automatic switch displays its pending value and reverts after a save failure', async () => {
  const response = Promise.withResolvers<UpdateStatus>()
  pending = response.promise
  mount()
  const user = userEvent.setup({ document, delay: null })
  await user.click(view().getByRole('switch'))
  await waitFor(() => assert.equal(view().getByRole('switch').hasAttribute('disabled'), true))
  assert.ok(view().getByRole('switch', { checked: true }))
  await act(async () => response.reject(new Error('Disk is full')))
  await waitFor(() => assert.equal(view().getByRole('switch').hasAttribute('disabled'), false))
  assert.ok(view().getByRole('switch', { checked: false }))
  assert.ok(view().getByRole('alert').textContent?.includes('Disk is full'))
})

for (const total of [2048, null]) {
  test(`download progress with ${total === null ? 'unknown' : 'known'} length has truthful accessibility`, () => {
    status = { ...status, phase: 'downloading', downloaded_bytes: 1024, total_bytes: total }
    mount()
    const progress = view().getByRole('progressbar', { name: 'Downloading update' })
    assert.equal(progress.getAttribute('aria-valuenow'), total === null ? null : '50')
    assert.ok(progress.getAttribute('aria-valuetext')?.includes(total === null ? 'total size unknown' : 'of 2.0 KB'))
    assert.equal(view().queryByText('100%'), null)
  })
}

test('failed check stays a failure rather than current or outdated and offers retry', () => {
  status = {
    ...status,
    last_checked_at: '2026-10-31T10:00:00Z',
    error: { stage: 'check', code: 'UPDATE_CHECK_FAILED', message: 'Could not connect' },
  }
  mount(
    <>
      <DesktopUpdates />
      <DesktopUpdateIndicator />
    </>
  )
  assert.ok(view().getByRole('alert').textContent?.includes('Could not connect'))
  assert.ok(view().getByRole('button', { name: 'Retry check' }))
  assert.equal(view().queryByText("You're up to date"), null)
  assert.equal(view().queryByRole('link', { name: 'Update available' }), null)
})

test('connection failure disables installation without hiding the verified stage', async () => {
  ready()
  mount()
  await act(async () => client.setQueryData(queryKeys.updates.connection(), 'failed'))
  await waitFor(() =>
    assert.equal(view().getByRole('button', { name: 'Install and restart' }).hasAttribute('disabled'), true)
  )
})

test('external and unsupported modes render native explanations without install actions', async () => {
  status = {
    ...status,
    mode: 'external',
    install_source: 'Debian package',
    reason: 'Use your package manager to update Pipedash.',
    available_version: '0.8.1',
    release_url: 'https://github.com/hcavarsan/pipedash/releases/tag/v0.8.1',
  }
  mount()
  assert.ok(view().getByRole('switch'))
  assert.ok(view().getByRole('button', { name: 'View release' }))
  assert.ok(view().getByText(status.reason ?? ''))
  assert.equal(view().queryByRole('button', { name: 'Install and restart' }), null)
  await act(async () =>
    applyUpdateStatus(client, {
      ...status,
      revision: 2,
      mode: 'unsupported',
      can_check: false,
      reason: 'Development builds do not use production updates.',
    })
  )
  await waitFor(() => assert.equal(view().queryByRole('switch'), null))
  assert.equal(view().queryByRole('button', { name: 'Check for updates' }), null)
  assert.ok(view().getByText('Development builds do not use production updates.'))
})

test('header indicator links directly to General and respects an existing dialog', async () => {
  ready()
  mount(<DesktopUpdateIndicator />)
  const user = userEvent.setup({ document, delay: null })
  const indicator = view().getByRole('link', { name: 'Restart to update' })
  assert.equal(indicator.getAttribute('href'), '/settings/general')
  const dialog = document.createElement('div')
  dialog.setAttribute('role', 'dialog')
  dialog.className = 'mantine-Modal-content'
  Object.defineProperty(dialog, 'getClientRects', { value: () => [{}] })
  document.body.append(dialog)
  await user.click(indicator)
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/pipelines')
  dialog.remove()
  await user.click(indicator)
  assert.equal(view().getByRole('status', { name: 'Location' }).textContent, '/settings/general')
  assert.equal(view().queryByRole('link', { name: 'Restart to update' }), null)
})
