import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../testing/requestSubmitShim'
import type { ConfigAnalysisResponse, MigrationPlan, MigrationResult, PipedashConfig } from '../../types'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { ConfigEditorModal } = await import('./ConfigEditorModal')
const { apiService } = await import('../../services')
const view = () => within(document.body)

const config: PipedashConfig = {
  general: { metrics_enabled: true, default_refresh_interval: 30, status_pages_enabled: false, proxy_url: '' },
  server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
  storage: { data_dir: '/tmp/pipedash-test', backend: 'sqlite' },
}

const plan = (dataDir: string): MigrationPlan => ({
  from: config,
  to: { ...config, storage: { ...config.storage, data_dir: dataDir } },
  steps: [],
  migrate_tokens: false,
  migrate_configs: true,
  migrate_cache: true,
  backend_changed: false,
  data_dir_changed: true,
  created_at: '2026-01-01T00:00:00Z',
})

const migrated: MigrationResult = {
  success: true,
  steps_completed: [],
  errors: [],
  duration_ms: 1,
  stats: {
    providers_migrated: 1,
    tokens_migrated: 0,
    cache_entries_migrated: 0,
    metrics_migrated: 0,
    permissions_migrated: 0,
  },
  provider_id_mapping: {},
}

let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
})

const restoreRequestSubmit = installRequestSubmitShim()

after(async () => {
  restoreRequestSubmit()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const renderEditor = async (onClose: () => void = () => undefined): Promise<void> => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(MemoryRouter, null, createElement(ConfigEditorModal, { onClose, initialContent: 'a = 1\n' }))
        )
      )
    )
  })
}

const editor = async (): Promise<HTMLTextAreaElement> => {
  const field = await view().findByRole('textbox', { name: 'config.toml' })
  assert.ok(field instanceof HTMLTextAreaElement)

  return field
}

test('the editor is locked while the explicit analysis is running', async (context) => {
  const user = userEvent.setup({ document })
  const analysis = Promise.withResolvers<ConfigAnalysisResponse>()
  context.mock.method(apiService, 'analyzeConfig', () => analysis.promise)
  context.mock.method(apiService, 'saveConfigContent', async () => undefined)
  await renderEditor()
  const field = await editor()

  await user.clear(field)
  await user.type(field, 'b = 2')
  await user.click(view().getByRole('button', { name: 'Save' }))

  await waitFor(() => assert.equal(field.disabled, true))
  await act(async () => analysis.resolve({ valid: true, errors: [], warnings: [] }))
})

test('migration confirmation keeps the editor locked and migrates the submitted plan and content', async (context) => {
  const user = userEvent.setup({ document })
  const submittedPlan = plan('/tmp/submitted')
  const migrations: MigrationPlan[] = []
  const saved: string[] = []
  context.mock.method(
    apiService,
    'analyzeConfig',
    async (): Promise<ConfigAnalysisResponse> => ({
      valid: true,
      errors: [],
      warnings: [],
      migration_plan: submittedPlan,
    })
  )
  context.mock.method(apiService, 'executeMigration', async (migrationPlan: MigrationPlan) => {
    migrations.push(migrationPlan)

    return migrated
  })
  context.mock.method(apiService, 'saveConfigContent', async (content: string) => {
    saved.push(content)
  })
  const closed = Promise.withResolvers<void>()
  await renderEditor(() => closed.resolve())
  const field = await editor()

  await user.clear(field)
  await user.type(field, 'b = 2')
  await user.click(view().getByRole('button', { name: 'Save' }))

  const confirm = await view().findByRole('button', { name: 'Migrate' })
  assert.equal(field.disabled, true)

  await user.click(confirm)
  await closed.promise

  assert.deepEqual(migrations, [submittedPlan])
  assert.deepEqual(saved, ['b = 2'])
})
