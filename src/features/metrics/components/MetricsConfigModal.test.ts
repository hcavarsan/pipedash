import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

const saves: Array<Record<string, unknown>> = []
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
      if (Number(args.retentionDays) < 1) {
        throw new Error('Retention days must be at least 1')
      }

      return null
    default:
      return null
  }
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { iosTheme } = await import('../../../theme/ios')
const { MetricsConfigModal } = await import('./MetricsConfigModal')
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const view = () => within(document.body)
const restoreRequestSubmit = installRequestSubmitShim()

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  saves.length = 0
})

after(async () => {
  restoreRequestSubmit()
  await GlobalRegistrator.unregister()
})

test('a blank custom retention is sent as given and the backend rejection is shown', async () => {
  const user = userEvent.setup({ document })

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
  )

  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), 'custom')
  const days = await view().findByRole('textbox', { name: 'Custom retention (days)' })
  const save = view().getByRole('button', { name: 'Save' })

  await user.clear(days)
  await user.click(save)
  await waitFor(() =>
    assert.deepEqual(
      saves.map((call) => call.retentionDays),
      [0]
    )
  )
  assert.ok((await view().findAllByText(/at least 1/i)).length > 0)

  await user.type(days, '45')
  await user.click(save)
  await waitFor(() =>
    assert.deepEqual(
      saves.map((call) => call.retentionDays),
      [0, 45]
    )
  )
})
