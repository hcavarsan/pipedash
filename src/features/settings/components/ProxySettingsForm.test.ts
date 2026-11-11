import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, {
  __TAURI_INTERNALS__: { invoke: async () => [] },
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { ProxySettingsForm } = await import('./ProxySettingsForm')
const view = () => within(document.body)

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function renderForm(onSave: (proxyUrl: string) => Promise<void>) {
  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        createElement(ProxySettingsForm, { savedProxy: '', mobileShell: false, pending: false, onSave })
      )
    )
  )
}

test('a rejected save shows its error only while the proxy value is the one that was submitted', async () => {
  const user = userEvent.setup({ document })
  const saved: string[] = []

  renderForm(async (proxyUrl) => {
    saved.push(proxyUrl)
    throw new Error('proxy refused')
  })

  const input = await view().findByRole('textbox', { name: 'Proxy URL' })
  await user.type(input, 'http://proxy.test:3128')
  await user.click(view().getByRole('button', { name: 'Save proxy' }))
  await view().findByText(/proxy refused/)

  await user.type(input, '9')
  await waitFor(() => assert.equal(view().queryAllByText(/proxy refused/).length, 0))
  assert.deepEqual(saved, ['http://proxy.test:3128'])
})
