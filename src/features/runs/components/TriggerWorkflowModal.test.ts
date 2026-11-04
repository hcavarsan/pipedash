import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { Pipeline, WorkflowParameter } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const state: { params: WorkflowParameter[]; paramsFailure: boolean; triggerFailure: boolean } = {
  params: [],
  paramsFailure: false,
  triggerFailure: false,
}
const triggers: Record<string, unknown>[] = []
Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case 'get_workflow_parameters':
          if (state.paramsFailure) {
            throw new Error('parameters unavailable')
          }

          return state.params
        case 'trigger_pipeline':
          triggers.push(args ?? {})

          if (state.triggerFailure) {
            throw new Error('provider rejected the trigger')
          }

          return JSON.stringify({ run_number: 42 })
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifications } = await import('@mantine/notifications')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { TriggerWorkflowModal } = await import('./TriggerWorkflowModal')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { DEFAULT_NOTIFICATION_PREFERENCES, useNotificationStore } = await import('../../../stores/notificationStore')
const view = () => within(document.body)

const branchParam: WorkflowParameter = {
  name: 'branch',
  label: 'Branch',
  description: null,
  type: 'string',
  required: true,
}

const pipeline: Pipeline = {
  id: 'github__1__owner/repo/build.yml',
  provider_id: 1,
  provider_type: 'github',
  name: 'Build',
  status: 'success',
  last_run: null,
  last_updated: '2026-10-25T00:00:00Z',
  repository: 'owner/repo',
  branch: 'main',
  workflow_file: 'build.yml',
}

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  state.params = [branchParam]
  state.paramsFailure = false
  state.triggerFailure = false
  triggers.length = 0
  useNotificationStore.setState({ preferences: DEFAULT_NOTIFICATION_PREFERENCES, notifyTriggeredRuns: true })
  mock.method(apiService, 'getWorkflowParameters', tauriService.getWorkflowParameters)
  mock.method(apiService, 'triggerPipeline', tauriService.triggerPipeline)
  notifyManager.setNotifyFunction((callback) => act(callback))
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  notifications.clean()
  mock.restoreAll()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

function renderModal(options: { initialInputs?: Record<string, unknown> } = {}) {
  const successes: Array<[string, number]> = []
  const closed = { value: false }

  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(TriggerWorkflowModal, {
          opened: true,
          pipeline,
          onClose: () => {
            closed.value = true
          },
          onSuccess: (id, runNumber) => successes.push([id, runNumber]),
          ...options,
        })
      )
    )
  )

  return { successes, closed }
}

test('clicking the footer button submits the typed parameters', async () => {
  const user = userEvent.setup({ document })
  const { successes, closed } = renderModal()

  const input = await view().findByRole('textbox', { name: 'Branch' })
  await user.type(input, 'release')
  await user.click(view().getByRole('button', { name: 'Trigger workflow' }))

  await waitFor(() => {
    assert.deepEqual(triggers, [{ params: { workflow_id: pipeline.id, inputs: { branch: 'release' } } }])
    assert.equal(closed.value, true)
    assert.deepEqual(successes, [[pipeline.id, 42]])
  })
})

test('a missing required value blocks the request and reports the field', async () => {
  const user = userEvent.setup({ document })
  const { closed } = renderModal()

  await view().findByRole('textbox', { name: /Branch/ })
  await user.click(view().getByRole('button', { name: 'Trigger workflow' }))

  assert.ok((await view().findAllByText('Branch is required')).length > 0)
  assert.equal(triggers.length, 0)
  assert.equal(closed.value, false)
})

test('replayed zero is shown in text fields', async () => {
  state.params = [{ name: 'retries', label: 'Retries', description: null, type: 'string', required: false }]
  renderModal({ initialInputs: { retries: 0 } })

  const input = await view().findByRole('textbox', { name: 'Retries' })
  assert.ok(input instanceof HTMLInputElement)
  assert.equal(input.value, '0')
})

test('a rejected trigger keeps the dialog open with the typed values', async () => {
  const user = userEvent.setup({ document })
  state.triggerFailure = true
  const { closed } = renderModal()

  const input = await view().findByRole('textbox', { name: 'Branch' })
  await user.type(input, 'release')
  await user.click(view().getByRole('button', { name: 'Trigger workflow' }))

  await waitFor(() => assert.equal(triggers.length, 1))
  assert.equal(closed.value, false)
  assert.ok(input instanceof HTMLInputElement)
  assert.equal(input.value, 'release')

  state.triggerFailure = false
  await user.click(view().getByRole('button', { name: 'Trigger workflow' }))
  await waitFor(() => assert.equal(closed.value, true))
})

test('a failed parameter load offers a retry that recovers', async () => {
  const user = userEvent.setup({ document })
  state.paramsFailure = true
  renderModal()

  await view().findByText("Couldn't load workflow parameters")
  state.paramsFailure = false
  await user.click(view().getByRole('button', { name: /retry|try again/i }))

  await view().findByRole('textbox', { name: 'Branch' })
})

test('a background parameter refetch does not overwrite a dirty draft', async () => {
  const user = userEvent.setup({ document })
  renderModal()

  const input = await view().findByRole('textbox', { name: 'Branch' })
  await user.type(input, 'release')

  state.params = [{ ...branchParam, label: 'Ref name' }]
  await act(async () => client.refetchQueries())

  await view().findByRole('textbox', { name: 'Ref name' })
  const refreshed = view().getByRole('textbox', { name: 'Ref name' })
  assert.ok(refreshed instanceof HTMLInputElement)
  assert.equal(refreshed.value, 'release')
})

const pendingFinishes = () => useNotificationStore.getState().preferences.watch_once.map((watch) => watch.pipeline_id)

async function triggerWithBranch() {
  const user = userEvent.setup({ document })

  await user.type(await view().findByRole('textbox', { name: 'Branch' }), 'release')
  await user.click(view().getByRole('button', { name: 'Trigger workflow' }))
}

test('a triggered run asks to be notified when it finishes by default', async () => {
  const { closed } = renderModal()

  const box = await view().findByRole('checkbox', { name: 'Notify me when it finishes' })
  assert.ok(box instanceof HTMLInputElement)
  assert.equal(box.checked, true)
  await triggerWithBranch()

  await waitFor(() => assert.equal(closed.value, true))
  assert.deepEqual(pendingFinishes(), [pipeline.id])
})

test('unchecking the box skips the request and is remembered for the next trigger', async () => {
  const user = userEvent.setup({ document })
  const { closed } = renderModal()

  await user.click(await view().findByRole('checkbox', { name: 'Notify me when it finishes' }))
  await triggerWithBranch()

  await waitFor(() => assert.equal(closed.value, true))
  assert.deepEqual(pendingFinishes(), [])
  assert.equal(useNotificationStore.getState().notifyTriggeredRuns, false)
})

test('a rejected trigger leaves no pending finish notification', async () => {
  state.triggerFailure = true
  renderModal()

  await triggerWithBranch()

  await waitFor(() => assert.equal(triggers.length, 1))
  assert.deepEqual(pendingFinishes(), [])
})

test('a watched pipeline needs no checkbox and gets no extra request', async () => {
  useNotificationStore.setState({
    preferences: { ...DEFAULT_NOTIFICATION_PREFERENCES, watched_pipelines: [pipeline.id] },
  })
  const { closed } = renderModal()

  await view().findByRole('textbox', { name: 'Branch' })
  assert.equal(view().queryByRole('checkbox', { name: 'Notify me when it finishes' }), null)
  await triggerWithBranch()

  await waitFor(() => assert.equal(closed.value, true))
  assert.deepEqual(pendingFinishes(), [])
})
