import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import { isRecord } from '../../utils/isRecord'

if (process.env.PIPEDASH_METRICS_CONFIG_TEST !== '1') {
  test('metrics settings validate retention on the mobile shell in an isolated runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_METRICS_CONFIG_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
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
  const { iosTheme } = await import('../../theme/ios')
  const { MetricsConfigModal } = await import('./MetricsConfigModal')
  const view = () => within(document.body)

  afterEach(async () => {
    await act(async () => cleanup())
    saves.length = 0
  })

  after(async () => {
    await GlobalRegistrator.unregister()
  })

  test('a blank or zero custom retention is rejected instead of being replaced by 7 days', async () => {
    const user = userEvent.setup({ document })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

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
    assert.ok(await view().findByText(/whole number from 1 to 90/i))
    assert.equal(save.hasAttribute('disabled'), true)

    await user.type(days, '0')
    assert.ok(await view().findByText(/whole number from 1 to 90/i))
    assert.equal(saves.length, 0)

    await user.clear(days)
    await user.type(days, '45')
    await waitFor(() => assert.equal(save.hasAttribute('disabled'), false))
    assert.equal(view().queryByText(/whole number from 1 to 90/i), null)
    assert.equal(saves.length, 0)
  })
}
