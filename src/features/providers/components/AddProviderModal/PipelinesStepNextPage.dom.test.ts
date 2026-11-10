import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment, type ReactNode, useReducer } from 'react'

import type { AvailablePipeline, PaginatedAvailablePipelines } from '../../../../types/pipelines'
import type { ProviderConfig } from '../../../../types/providers'

import { pluginFixture } from './pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../../services/api')
const { tauriService } = await import('../../../../services/tauri')
const { formReducer } = await import('./reducer')
const { initialFormState } = await import('./types')
const { PipelinesStep } = await import('./PipelinesStep')

const view = () => within(document.body)
let client = new QueryClient()

const providerConfig: ProviderConfig = {
  name: 'Stub',
  provider_type: 'stub',
  token: '',
  config: {},
  refresh_interval: 300,
}

const job = (id: string): AvailablePipeline => ({
  id,
  name: id,
  description: null,
  organization: 'org',
  repository: 'org/repo',
})

const page = (ids: string[], number: number, hasMore: boolean): PaginatedAvailablePipelines => ({
  items: ids.map(job),
  page: number,
  page_size: 2,
  total_count: 4,
  total_pages: 2,
  has_more: hasMore,
})

function Picker(): ReactNode {
  const [state, dispatch] = useReducer(formReducer, {
    ...initialFormState,
    step: 'pipelines',
    authMode: 'none',
    selectedPlugin: pluginFixture(),
  })

  return createElement(
    Fragment,
    null,
    createElement(PipelinesStep, {
      state,
      dispatch,
      providerConfig,
      terms: pluginFixture().presentation.terminology,
      isMobile: false,
    }),
    createElement('output', { 'aria-label': 'Selected' }, [...state.selectedPipelines].sort().join(','))
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('a rejected next page keeps the loaded rows and selection, and Retry requests that page', async (context) => {
  const user = userEvent.setup({ document })
  const requested: number[] = []
  let failSecond = true
  const preview = async (
    _type: string,
    _config: Record<string, string>,
    _org?: string,
    _search?: string,
    pageNumber?: number
  ): Promise<PaginatedAvailablePipelines> => {
    requested.push(pageNumber ?? 1)
    if (pageNumber === 2) {
      if (failSecond) {
        throw new Error('page two offline')
      }

      return page(['c', 'd'], 2, false)
    }

    return page(['a', 'b'], 1, true)
  }
  for (const service of [apiService, tauriService]) {
    context.mock.method(service, 'fetchProviderOrganizations', async () => [
      { id: 'org', name: 'org', description: null },
    ])
    context.mock.method(service, 'previewProviderPipelines', preview)
    context.mock.method(service, 'getMatchingPipelineIds', async () => ['a', 'b', 'c', 'd'])
  }
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  notifyManager.setNotifyFunction((callback) => act(callback))
  render(createElement(MantineProvider, null, createElement(QueryClientProvider, { client }, createElement(Picker))))

  assert.ok(await view().findByText('page two offline'))
  assert.ok(view().getByText(/^2 .* listed$/))
  await user.click(view().getByRole('checkbox', { name: 'Select all' }))
  await waitFor(() => assert.equal(view().getByRole('status', { name: 'Selected' }).textContent, 'a,b,c,d'))
  assert.deepEqual(requested, [1, 2])

  failSecond = false
  await user.click(view().getByRole('button', { name: 'Retry' }))

  await waitFor(() => assert.ok(view().getByText(/^4 .* listed$/)))
  assert.deepEqual(requested, [1, 2, 2])
  assert.equal(view().queryByText('page two offline'), null)
  assert.equal(view().getByRole('status', { name: 'Selected' }).textContent, 'a,b,c,d')
})
