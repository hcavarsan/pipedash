import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { AggregatedMetrics } from '../../types'

GlobalRegistrator.register({ width: 1400, height: 900 })

const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { MetricsDataTable } = await import('./MetricsDataTable')

const view = () => within(document.body)

const DATA: AggregatedMetrics = {
  metric_type: 'run_duration',
  aggregation_period: 'hourly',
  total_count: 300,
  metrics: Array.from({ length: 300 }, (_, index) => ({
    timestamp: new Date(Date.UTC(2026, 0, 1, index)).toISOString(),
    value: index + 1,
    count: 1,
    min: index + 1,
    max: index + 1,
    avg: index + 1,
  })),
}

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  if (clientHeight) {
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
  } else {
    Reflect.deleteProperty(HTMLElement.prototype, 'clientHeight')
  }
  await GlobalRegistrator.unregister()
})

test('a long series mounts only a window of rows, and a sorted row selects its original data point', async () => {
  const user = userEvent.setup({ document })
  const selected: Array<number | null> = []

  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        MemoryRouter,
        null,
        createElement(MetricsDataTable, {
          data: DATA,
          selectedIndex: null,
          onRowClick: (index) => selected.push(index),
        })
      )
    )
  )

  assert.ok(view().getAllByRole('row').length < 60)

  const header = view().getByRole('columnheader', { name: /Duration/ })
  await user.click(within(header).getByRole('button'))
  await user.click(within(header).getByRole('button'))

  const [first] = view().getAllByRole('button', { pressed: false })
  assert.ok(first)
  await user.click(first)
  assert.deepEqual(selected, [299])
})
