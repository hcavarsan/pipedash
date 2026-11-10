import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'
import type { MigrationOptions, MigrationPlan, PipedashConfig } from '../../../types/storage'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MigrationConfirmModal } = await import('./MigrationConfirmModal')
const view = () => within(document.body)

const config: PipedashConfig = {
  general: { metrics_enabled: true, default_refresh_interval: 30, status_pages_enabled: false, proxy_url: '' },
  server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
  storage: { data_dir: '/tmp/pipedash-test', backend: 'sqlite' },
}

const plan: MigrationPlan = {
  from: config,
  to: config,
  steps: [],
  migrate_tokens: true,
  migrate_configs: true,
  migrate_cache: true,
  backend_changed: false,
  data_dir_changed: true,
  created_at: '2026-01-01T00:00:00Z',
}

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function renderModal(onConfirm: (options: MigrationOptions) => Promise<void>) {
  render(
    createElement(
      MantineProvider,
      null,
      createElement(MigrationConfirmModal, { onClose: () => undefined, migrationPlan: plan, onConfirm })
    )
  )
}

test('a rejected migration shows its error until the password or the migrate choice changes', async () => {
  const user = userEvent.setup({ document })
  const attempts: MigrationOptions[] = []

  renderModal(async (options) => {
    attempts.push(options)
    throw new Error('wrong vault password')
  })

  const password = await view().findByLabelText(/Vault password/)
  await user.type(password, 'secret')
  await user.click(view().getByRole('button', { name: 'Migrate' }))
  await view().findByText(/wrong vault password/)

  await user.type(password, '1')
  await waitFor(() => assert.equal(view().queryAllByText(/wrong vault password/).length, 0))

  await user.click(view().getByRole('button', { name: 'Migrate' }))
  await view().findByText(/wrong vault password/)

  await user.click(view().getByRole('checkbox', { name: /Migrate existing data/ }))
  await waitFor(() => assert.equal(view().queryAllByText(/wrong vault password/).length, 0))
  assert.equal(attempts.length, 2)
})
