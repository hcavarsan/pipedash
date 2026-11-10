import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import type { UserEvent } from '@testing-library/user-event'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

const saves: Array<Record<string, unknown>> = []
const flushes: Array<Record<string, unknown>> = []
let saveFailure: string | null = null
let flushFailure: string | null = null

mockIPC((command, args) => {
  switch (command) {
    case 'get_pipeline_metrics_config':
      return { pipeline_id: 'p1', enabled: true, retention_days: 30 }
    case 'get_metrics_storage_stats':
      return {
        total_metrics_count: 0,
        estimated_size_bytes: 0,
        estimated_size_mb: 0,
        last_cleanup_at: null,
        by_pipeline: [],
      }
    case 'update_pipeline_metrics_config':
      assert.ok(isRecord(args))
      saves.push(args)
      if (saveFailure !== null) {
        throw new Error(saveFailure)
      }
      if (Number(args.retentionDays) < 1) {
        throw new Error('Retention days must be at least 1')
      }

      return null
    case 'flush_pipeline_metrics':
      assert.ok(isRecord(args))
      flushes.push(args)
      if (flushFailure !== null) {
        throw new Error(flushFailure)
      }

      return 3
    default:
      return null
  }
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ModalsProvider } = await import('@mantine/modals')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { notifications } = await import('../../../lib/notifications')
const { iosTheme } = await import('../../../theme/ios')
const { MetricsConfigModal } = await import('./MetricsConfigModal')
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const view = () => within(document.body)
const restoreRequestSubmit = installRequestSubmitShim()

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  saves.length = 0
  flushes.length = 0
  saveFailure = null
  flushFailure = null
  mock.restoreAll()
})

after(async () => {
  restoreRequestSubmit()
  await GlobalRegistrator.unregister()
})

function renderModal(handlers: { onClose?: () => void; onConfigChange?: () => void } = {}) {
  render(
    createElement(
      MantineProvider,
      { theme: iosTheme, env: 'test' },
      createElement(
        QueryClientProvider,
        { client },
        createElement(
          MemoryRouter,
          null,
          createElement(
            ModalsProvider,
            null,
            createElement(MetricsConfigModal, {
              scope: 'pipeline',
              pipelineId: 'p1',
              pipelineName: 'Build',
              opened: true,
              onClose: handlers.onClose ?? (() => undefined),
              ...(handlers.onConfigChange ? { onConfigChange: handlers.onConfigChange } : {}),
            })
          )
        )
      )
    )
  )
}

function errorToastTitles(shown: { mock: { calls: Array<{ arguments: unknown[] }> } }): unknown[] {
  return shown.mock.calls
    .map((call) => call.arguments[0])
    .filter((data) => isRecord(data) && data.color === 'red')
    .map((data) => (isRecord(data) ? data.title : undefined))
}

async function disableAndSave(user: UserEvent): Promise<void> {
  await user.click(await view().findByLabelText('Enable metrics'))
  await user.click(view().getByRole('button', { name: 'Save' }))
  await user.click(await view().findByRole('button', { name: 'Disable and delete data' }))
}

test('a blank custom retention is rejected before saving and a valid one is sent', async () => {
  const user = userEvent.setup({ document })

  renderModal()

  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), 'custom')
  const days = await view().findByRole('textbox', { name: 'Custom retention (days)' })
  const save = view().getByRole('button', { name: 'Save' })

  await user.clear(days)
  await user.click(save)
  assert.ok((await view().findAllByText(/enter a number/i)).length > 0)
  assert.equal(saves.length, 0)

  await user.type(days, '45')
  await user.click(save)
  await waitFor(() =>
    assert.deepEqual(
      saves.map((call) => call.retentionDays),
      [45]
    )
  )
})

test('disabling with a blank retention is blocked before saving or deleting', async () => {
  const user = userEvent.setup({ document })

  renderModal()

  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), 'custom')
  await user.clear(await view().findByRole('textbox', { name: 'Custom retention (days)' }))
  await user.click(await view().findByLabelText('Enable metrics'))
  await user.click(view().getByRole('button', { name: 'Save' }))

  assert.ok((await view().findAllByText(/enter a number/i)).length > 0)
  assert.deepEqual(saves, [])
  assert.deepEqual(flushes, [])
})

test('a failed save shows the inline error once and never deletes stored metrics', async () => {
  const user = userEvent.setup({ document })
  const shown = mock.method(notifications, 'show')
  const onClose = mock.fn()

  saveFailure = 'Disk is full'
  renderModal({ onClose })
  await disableAndSave(user)

  assert.ok((await view().findAllByText(/disk is full/i)).length > 0)
  assert.deepEqual(flushes, [])
  assert.deepEqual(errorToastTitles(shown), [])
  assert.equal(onClose.mock.callCount(), 0)
})

test('a failed delete after a saved disable stays open and retries only the delete', async () => {
  const user = userEvent.setup({ document })
  const shown = mock.method(notifications, 'show')
  const onClose = mock.fn()
  const onConfigChange = mock.fn()

  flushFailure = 'Store is offline'
  renderModal({ onClose, onConfigChange })
  await disableAndSave(user)

  assert.ok((await view().findAllByText(/store is offline/i)).length > 0)
  assert.ok(view().getByText("Couldn't delete metrics"))
  assert.equal(onClose.mock.callCount(), 0)
  assert.equal(onConfigChange.mock.callCount(), 1)
  assert.deepEqual(errorToastTitles(shown), [])

  flushFailure = null
  await user.click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.equal(onClose.mock.callCount(), 1))
  assert.equal(saves.length, 1)
  assert.equal(flushes.length, 2)
})

test('a rejected save hides its error as soon as the draft changes, without a new submit', async () => {
  const user = userEvent.setup({ document })

  saveFailure = 'Disk is full'
  renderModal()

  await user.click(await view().findByRole('button', { name: 'Save' }))
  assert.ok((await view().findAllByText(/disk is full/i)).length > 0)

  await user.selectOptions(view().getByRole('combobox', { name: 'Retention period' }), 'custom')
  await user.type(await view().findByRole('textbox', { name: 'Custom retention (days)' }), '5')

  await waitFor(() => assert.equal(view().queryAllByText(/disk is full/i).length, 0))
  assert.equal(saves.length, 1)
})
