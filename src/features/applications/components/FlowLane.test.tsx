import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { copyFixture, flowFixture } from './applicationFixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { flowSize, MIN_SCALE, nodePosition } = await import('../lib/flowLayout')
const { FlowLane } = await import('./FlowLane')

const client = new QueryClient()

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(
    MantineProvider,
    { env: 'test' },
    createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, children))
  )

test('nodes are placed at their backend column and row on the 180x72 grid with 40 px gaps', () => {
  const flow = flowFixture()
  const view = render(<FlowLane flow={flow} copy={copyFixture()} applicationId={1} canEdit />, { wrapper })

  for (const node of flow.nodes) {
    const { x, y } = nodePosition(node, false)
    const group = view.container.querySelector(`g[data-node-id="${node.id}"]`)

    assert.equal(group?.getAttribute('transform'), `translate(${x} ${y})`)
  }
  assert.deepEqual(nodePosition({ column: 2, row: 1 }, false), { x: 440, y: 112 })
})

test('vertical layout swaps the axes so columns stack top to bottom', () => {
  assert.deepEqual(nodePosition({ column: 2, row: 1 }, true), { x: 220, y: 224 })
})

test('the svg scales to its container up to the natural grid width and scrolls only below the minimum scale', () => {
  const flow = flowFixture()
  const view = render(<FlowLane flow={flow} copy={copyFixture()} applicationId={1} canEdit />, { wrapper })
  const svg = view.container.querySelector('svg')
  const { width, height } = flowSize(flow, false)

  assert.equal(svg?.getAttribute('viewBox'), `0 0 ${width} ${height}`)
  assert.equal(svg?.hasAttribute('width'), false)
  assert.equal(svg?.style.width, '100%')
  assert.equal(svg?.style.maxWidth, `${width}px`)
  assert.equal(svg?.style.height, 'auto')
  assert.equal(svg?.style.minWidth, `${width * MIN_SCALE}px`)
})

test('suggested links are dashed and exact links are solid, each with a focusable button', () => {
  const view = render(<FlowLane flow={flowFixture()} copy={copyFixture()} applicationId={1} canEdit />, { wrapper })

  assert.equal(view.container.querySelector('path[data-link-id="l1"]')?.hasAttribute('stroke-dasharray'), false)
  assert.ok(view.container.querySelector('path[data-link-id="l2"]')?.getAttribute('stroke-dasharray'))
  assert.equal(view.getAllByRole('button', { name: 'why_linked' }).length, 2)
})

test('an empty flow renders the empty copy and no svg', () => {
  const flow = { columns: 0, rows: 0, nodes: [], links: [] }
  const view = render(<FlowLane flow={flow} copy={copyFixture()} applicationId={1} canEdit />, { wrapper })

  assert.ok(view.getByText('flow_empty'))
  assert.equal(view.container.querySelector('svg'), null)
})
