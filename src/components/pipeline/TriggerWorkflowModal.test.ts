import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { Pipeline } from '../../types'

GlobalRegistrator.register({ width: 1400, height: 900 })

const triggers: Record<string, unknown>[] = []
Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case 'get_workflow_parameters':
          return [{ name: 'branch', label: 'Branch', description: null, type: 'string', required: true }]
        case 'trigger_pipeline':
          triggers.push(args ?? {})
          return JSON.stringify({ run_number: 42 })
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifications } = await import('@mantine/notifications')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { TriggerWorkflowModal } = await import('./TriggerWorkflowModal')
const { apiService, tauriService } = await import('../../services')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifications.clean()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('the workflow footer submits its parameter form through the native submit button', async (context) => {
  context.mock.method(apiService, 'getWorkflowParameters', tauriService.getWorkflowParameters)
  context.mock.method(apiService, 'triggerPipeline', tauriService.triggerPipeline)
  notifyManager.setNotifyFunction((callback) => act(callback))
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
  const successes: Array<[string, number]> = []
  let closed = false
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
            closed = true
          },
          onSuccess: (id, runNumber) => successes.push([id, runNumber]),
        })
      )
    )
  )

  const input = await view().findByRole('textbox', { name: 'Branch' })
  fireEvent.change(input, { target: { value: 'release' } })
  const form = input.closest('form')
  assert.ok(form)
  const submit = view().getByRole('button', { name: 'Trigger workflow' })
  assert.ok(submit instanceof HTMLButtonElement)
  assert.equal(submit.type, 'submit')
  assert.equal(submit.form, form)
  assert.equal(form.contains(submit), false)

  act(() => form.requestSubmit(submit))
  await waitFor(() => {
    assert.deepEqual(triggers, [{ params: { workflow_id: pipeline.id, inputs: { branch: 'release' } } }])
    assert.equal(closed, true)
    assert.deepEqual(successes, [[pipeline.id, 42]])
  })
})
