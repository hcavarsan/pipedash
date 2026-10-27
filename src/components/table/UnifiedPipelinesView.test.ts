import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { Pipeline, PipelineStatus, ProviderSummary } from '../../types'

GlobalRegistrator.register({ width: 1400, height: 900 })

// Import after the DOM exists so Mantine and the stores initialize against it.
const { act, cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { UnifiedPipelinesView } = await import('./UnifiedPipelinesView')
const { pinKey } = await import('../../stores/pinnedStore')
const { useHiddenStore } = await import('../../stores/hiddenStore')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

const provider: ProviderSummary = {
  id: 1,
  name: 'GitHub',
  provider_type: 'github',
  icon: null,
  pipeline_count: 2,
  last_updated: null,
  refresh_interval: 30,
  configured_repositories: ['acme/app'],
  last_fetch_status: 'success',
  last_fetch_error: null,
  last_fetch_error_kind: null,
  last_fetch_at: null,
  paused_until: null,
  health: { level: 'operational', signals: [], next_attempt_at: null, in_flight: false, status_page: null },
}

const workflow = (name: string, status: PipelineStatus): Pipeline => ({
  id: `github__1__acme/app/${name}.yml`,
  provider_id: 1,
  provider_type: 'github',
  name,
  repository: 'acme/app',
  branch: 'main',
  workflow_file: `${name}.yml`,
  status,
  last_run: null,
  last_updated: '2026-10-27T00:00:00Z',
})

const build = workflow('build', 'success')
const legacy = workflow('legacy', 'failed')

notifyManager.setNotifyFunction((callback) => act(callback))

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  useHiddenStore.setState(useHiddenStore.getInitialState(), true)
})

after(() => {
  notifyManager.setNotifyFunction((callback) => callback())
  GlobalRegistrator.unregister()
})

async function repositoryStatus(pipelines: Pipeline[], page: string) {
  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(
          MemoryRouter,
          { initialEntries: [page] },
          createElement(UnifiedPipelinesView, {
            pipelines,
            providers: [provider],
            onRetry: () => {},
            onTrigger: () => {},
            onViewHistory: () => {},
            onViewRun: () => {},
          })
        )
      )
    )
  )
  const row = (await view().findByRole('button', { name: 'app' })).closest('tr')

  assert.ok(row)

  return row.textContent ?? ''
}

test('a hidden failed workflow stays out of the repository status with Show hidden on', async () => {
  useHiddenStore.setState({ hiddenKeys: [pinKey(legacy, provider.name)] })

  const status = await repositoryStatus([build, legacy], '/pipelines?hidden=show')

  assert.match(status, /Success/)
  assert.doesNotMatch(status, /Failed/)
})

test('a repository with only hidden workflows keeps their status with Show hidden on', async () => {
  useHiddenStore.setState({ hiddenKeys: [pinKey(legacy, provider.name)] })

  const status = await repositoryStatus([legacy], '/pipelines?hidden=show')

  assert.match(status, /Failed/)
})
