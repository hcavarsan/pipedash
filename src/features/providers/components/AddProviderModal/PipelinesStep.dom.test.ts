import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment, type ReactNode, useReducer } from 'react'

import type { AvailablePipeline, PaginatedAvailablePipelines, ProviderConfig } from '../../../../types/domain'

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
  name: 'Prow',
  provider_type: 'prow',
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

const page = (items: AvailablePipeline[], number: number, hasMore: boolean): PaginatedAvailablePipelines => ({
  items,
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
    selectedPlugin: pluginFixture({ name: 'Prow', provider_type: 'prow' }),
  })

  return createElement(
    Fragment,
    null,
    state.step === 'pipelines' && createElement(PipelinesStep, { state, dispatch, providerConfig, isMobile: false }),
    createElement(
      'button',
      { type: 'button', onClick: () => dispatch({ type: 'SET_STEP', step: 'credentials' }) },
      'Back'
    ),
    createElement('output', { 'aria-label': 'Selected' }, [...state.selectedPipelines].sort().join(',')),
    createElement('output', { 'aria-label': 'Selecting' }, String(state.selectingAll))
  )
}

function mockDeck(context: TestContext): { releaseSecondPage: () => void; secondPageRequests: () => number } {
  const waiting: (() => void)[] = []
  let requests = 0
  const organizations = async () => [{ id: 'org', name: 'org', description: null }]
  const preview = async (
    _type: string,
    _config: Record<string, string>,
    _org?: string,
    search?: string,
    pageNumber?: number
  ): Promise<PaginatedAvailablePipelines> => {
    if (search) {
      return page([], 1, false)
    }
    if (pageNumber !== 2) {
      return page([job('a'), job('b')], 1, true)
    }
    requests += 1
    const { promise, resolve } = Promise.withResolvers<void>()
    waiting.push(resolve)
    await promise
    return page([job('c'), job('d')], 2, false)
  }

  for (const service of [apiService, tauriService]) {
    context.mock.method(service, 'fetchProviderOrganizations', organizations)
    context.mock.method(service, 'previewProviderPipelines', preview)
  }

  return {
    releaseSecondPage: () => {
      for (const release of waiting.splice(0)) {
        release()
      }
    },
    secondPageRequests: () => requests,
  }
}

function renderPicker(): void {
  render(createElement(MantineProvider, null, createElement(QueryClientProvider, { client }, createElement(Picker))))
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

test('select all loads the remaining pages and selects every matching pipeline', async (context) => {
  const user = userEvent.setup({ document })
  const deck = mockDeck(context)
  renderPicker()

  await user.click(await view().findByRole('checkbox', { name: 'Select all' }))
  await waitFor(() => assert.ok(deck.secondPageRequests() > 0))
  await act(async () => deck.releaseSecondPage())

  await waitFor(() => assert.equal(view().getByRole('status', { name: 'Selected' }).textContent, 'a,b,c,d'))
})

test('changing the search while select all waits for a page selects nothing', async (context) => {
  const user = userEvent.setup({ document })
  const deck = mockDeck(context)
  renderPicker()

  await user.click(await view().findByRole('checkbox', { name: 'Select all' }))
  await waitFor(() => assert.ok(deck.secondPageRequests() > 0))
  await user.type(view().getByRole('textbox', { name: 'Search pipelines' }), 'x')
  await act(async () => deck.releaseSecondPage())
  await waitFor(() => assert.equal(view().queryAllByText(/Selecting all/).length, 0))

  assert.equal(view().getByRole('status', { name: 'Selected' }).textContent, '')
})

test('going back while select all waits for a page selects nothing', async (context) => {
  const user = userEvent.setup({ document })
  const deck = mockDeck(context)
  renderPicker()

  await user.click(await view().findByRole('checkbox', { name: 'Select all' }))
  await waitFor(() => assert.ok(deck.secondPageRequests() > 0))
  await user.click(view().getByRole('button', { name: 'Back' }))
  await act(async () => deck.releaseSecondPage())
  await waitFor(() => assert.equal(view().getByRole('status', { name: 'Selecting' }).textContent, 'false'))

  assert.equal(view().getByRole('status', { name: 'Selected' }).textContent, '')
})
