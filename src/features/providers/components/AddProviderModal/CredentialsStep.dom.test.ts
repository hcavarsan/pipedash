import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode, useReducer, useRef } from 'react'

import type { PluginMetadata } from '../../../../types/plugins'

import { configFieldFixture, pluginFixture } from './pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { formReducer } = await import('./reducer')
const { initialFormState } = await import('./types')
const { CredentialsStep } = await import('./CredentialsStep')

const view = () => within(document.body)
let client = new QueryClient()

const multiMethod = pluginFixture({
  config_schema: {
    auth_options: [
      { mode: 'token', label: 'Key', description: 'Sends a key with every request.' },
      { mode: 'sign_in', label: 'Browser', description: 'Signs in with a browser.' },
      { mode: 'kubeconfig', label: 'Cluster', description: 'Reads a cluster file.' },
    ],
    fields: [
      configFieldFixture({
        key: 'server_url',
        label: 'Server URL',
        field_type: 'Text',
        required: false,
        section: 'connection',
        auth_modes: ['token', 'sign_in'],
      }),
      configFieldFixture({ key: 'token', label: 'Access key', auth_modes: ['token'] }),
      configFieldFixture({
        key: 'kubeconfig',
        label: 'Cluster file',
        field_type: 'Text',
        required: false,
        auth_modes: ['kubeconfig'],
      }),
    ],
  },
})

function Step({ plugin }: { plugin: PluginMetadata }): ReactNode {
  const [state, dispatch] = useReducer(formReducer, plugin, (selected) =>
    formReducer(
      { ...initialFormState, step: 'credentials' },
      { type: 'SELECT_PLUGIN', plugin: selected, isEditMode: false }
    )
  )
  const formRef = useRef<HTMLFormElement | null>(null)

  return createElement(
    'form',
    { ref: formRef },
    createElement(CredentialsStep, {
      state,
      dispatch,
      isMobile: false,
      submitting: false,
      formRef,
      formId: 'credentials',
      validateKey: async () => undefined,
      missingLabels: [],
    })
  )
}

function renderStep(plugin: PluginMetadata): void {
  render(
    createElement(
      MantineProvider,
      null,
      createElement(QueryClientProvider, { client }, createElement(Step, { plugin }))
    )
  )
}

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  notifyManager.setNotifyFunction((callback) => act(callback))
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('the control lists every declared method and describes the selected one', async () => {
  const user = userEvent.setup({ document })
  renderStep(multiMethod)

  const group = view().getByRole('radiogroup', { name: 'Authentication method' })
  const labels = within(group)
    .getAllByRole('radio')
    .map((radio) => radio.getAttribute('value'))

  assert.deepEqual(labels, ['token', 'sign_in', 'kubeconfig'])
  assert.ok(view().getByText('Key'))
  assert.ok(view().getByText('Browser'))
  assert.ok(view().getByText('Sends a key with every request.'))
  assert.equal(view().queryByText('Reads a cluster file.'), null)

  await user.click(within(group).getByRole('radio', { name: 'Cluster' }))

  assert.ok(view().getByText('Reads a cluster file.'))
  assert.equal(view().queryByText('Sends a key with every request.'), null)
})

test('choosing a method shows only the fields declared for it', async () => {
  const user = userEvent.setup({ document })
  renderStep(multiMethod)

  assert.ok(view().getByRole('textbox', { name: 'Server URL' }))
  assert.ok(view().getByLabelText(/^Access key/))
  assert.equal(view().queryByRole('textbox', { name: 'Cluster file' }), null)

  await user.click(view().getByRole('radio', { name: 'Cluster' }))

  assert.equal(view().queryByRole('textbox', { name: 'Server URL' }), null)
  assert.equal(view().queryByLabelText(/^Access key/), null)
  assert.ok(view().getByRole('textbox', { name: 'Cluster file' }))
})

test('a provider with one method renders no method control', () => {
  renderStep(pluginFixture())

  assert.equal(view().queryByRole('radiogroup'), null)
  assert.ok(view().getByLabelText(/^Token/))
})
