import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 1400, height: 900 })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'macos' } })

const savedRetention: unknown[] = []

mockIPC((command, args) => {
  if (command === 'get_pipeline_metrics_config') {
    return { pipeline_id: 'p1', enabled: true, retention_days: 30 }
  }
  if (command === 'get_metrics_storage_stats') {
    return {
      total_metrics_count: 0,
      estimated_size_bytes: 0,
      estimated_size_mb: 0,
      last_cleanup_at: null,
      by_pipeline: [],
    }
  }
  if (command === 'update_pipeline_metrics_config' && isRecord(args)) {
    savedRetention.push(args.retentionDays)
  }

  return null
})

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MetricsConfigModal } = await import('./MetricsConfigModal')
const restoreRequestSubmit = installRequestSubmitShim()

after(async () => {
  await act(async () => cleanup())
  restoreRequestSubmit()
  await GlobalRegistrator.unregister()
})

test('enter in the custom retention field submits the dialog with the typed days', async () => {
  const user = userEvent.setup({ document })
  const view = within(document.body)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  render(
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(
        QueryClientProvider,
        { client },
        createElement(MetricsConfigModal, {
          scope: 'pipeline',
          pipelineId: 'p1',
          pipelineName: 'Build',
          opened: true,
          onClose: () => undefined,
        })
      )
    )
  )

  await user.click(await view.findByRole('combobox', { name: 'Retention period' }))
  await user.click(await view.findByRole('option', { name: 'Custom', hidden: true }))
  const days = await view.findByRole('textbox', { name: 'Custom retention (days)' })
  await user.clear(days)
  await user.type(days, '45')
  assert.ok(days instanceof HTMLInputElement)

  const browserSubmits = fireEvent.keyDown(days, { key: 'Enter' })
  if (browserSubmits) {
    days.form?.requestSubmit()
  }

  assert.equal(browserSubmits, true)
  await waitFor(() => assert.deepEqual(savedRetention, [45]))
})
