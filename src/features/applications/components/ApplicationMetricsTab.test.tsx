import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { copyFixture, metricFixture, metricsFixture } from './applicationFixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { ApplicationMetricsTab } = await import('./ApplicationMetricsTab')

let client = new QueryClient()

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  mock.restoreAll()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(MantineProvider, { env: 'test' }, createElement(QueryClientProvider, { client }, children))

test('a null value renders the reason and no number, with coverage under every card', async () => {
  const unavailable = metricFixture({
    id: 'wait_time',
    label: 'Wait time',
    value: null,
    unavailable_reason: 'no recorded start',
    coverage: { counted: 0, excluded: 4, since: null, label: 'excluded 4' },
  })
  const available = metricFixture({ id: 'build_time', label: 'Build time', value: 90 })

  mock.method(apiService, 'getApplicationMetrics', async () => metricsFixture([unavailable, available]))

  const view = render(<ApplicationMetricsTab applicationId={1} copy={copyFixture()} />, { wrapper })

  await waitFor(() => assert.ok(view.getByText('no recorded start')))

  const card = view.container.querySelector('[data-metric="wait_time"]')

  assert.ok(card)
  assert.equal(/\d/.test((card.textContent ?? '').replace('excluded 4', '').replace('Wait time', '')), false)
  assert.ok(view.getByText('excluded 4'))
  assert.ok(view.getByText('covers 1'))
  assert.ok(view.container.querySelector('[data-metric="build_time"]')?.textContent?.includes('1m'))
})
