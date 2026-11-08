import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { dashboardFixture } from '../../../testing/metrics'

GlobalRegistrator.register({ width: 1400, height: 900 })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('aria-rowindex') ? 48 : 0
  },
})

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { MetricsDataTable } = await import('./MetricsDataTable')

const view = () => within(document.body)

const DATA = dashboardFixture({
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
})
const SORTED = { ...DATA, table_indices: DATA.table_indices.toReversed() }

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, name)
    }
  }
  await GlobalRegistrator.unregister()
})

test('a long series mounts only a window of rows, and a sorted row selects its original data point', async () => {
  const user = userEvent.setup({ document })
  const selected: Array<number | null> = []
  const sorts: Array<string | undefined> = []

  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        MemoryRouter,
        null,
        createElement(MetricsDataTable, {
          data: SORTED,
          sort: undefined,
          onSortChange: (next) => sorts.push(next?.direction),
          selectedIndex: null,
          onRowClick: (index) => selected.push(index),
        })
      )
    )
  )

  assert.ok(view().getAllByRole('row').length < 60)

  const header = view().getByRole('columnheader', { name: /Duration/ })
  await user.click(within(header).getByRole('button'))
  assert.deepEqual(sorts, ['asc'])

  const [first] = view().getAllByRole('button', { pressed: false })
  assert.ok(first)
  await user.click(first)
  assert.deepEqual(selected, [299])
})
