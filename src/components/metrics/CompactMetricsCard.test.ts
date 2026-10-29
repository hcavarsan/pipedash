import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { CompactMetricsCard } = await import('./CompactMetricsCard')

const view = () => within(document.body)

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a crashing chart shows its fallback outside the expand button and the card still expands', async () => {
  const user = userEvent.setup({ document })
  let expanded = 0
  const brokenData = JSON.parse(
    JSON.stringify({
      metric_type: 'unsupported',
      aggregation_period: 'hourly',
      total_count: 1,
      metrics: [{ timestamp: '2026-10-25T12:00:00Z', value: 42, count: 1, min: 42, max: 42, avg: 42 }],
    })
  )

  render(
    createElement(
      MantineProvider,
      null,
      createElement(CompactMetricsCard, {
        metricType: 'run_duration',
        data: brokenData,
        title: 'Duration',
        loading: false,
        onExpand: () => {
          expanded += 1
        },
      })
    )
  )

  const retry = await view().findByRole('button', { name: 'Try again' })
  assert.equal(retry.parentElement?.closest('button'), null)

  await user.click(view().getByRole('button', { name: 'Expand Duration' }))
  assert.equal(expanded, 1)
})
