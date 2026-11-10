import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { providerSummaryFixture, viewPipelineFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const CARD_ROW_HEIGHT = 130
let containerWidth = 1200
const resizeCallbacks = new Set<() => void>()

class ObserverStub {
  private readonly notify: () => void

  constructor(callback: ResizeObserverCallback) {
    this.notify = () => callback([], this)
  }

  observe(): void {
    resizeCallbacks.add(this.notify)
  }

  unobserve(): void {}

  disconnect(): void {
    resizeCallbacks.delete(this.notify)
  }
}

Object.assign(window, { ResizeObserver: ObserverStub })
Object.assign(globalThis, { ResizeObserver: ObserverStub })
Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => containerWidth })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', { configurable: true, get: () => CARD_ROW_HEIGHT })

const { act, cleanup, render } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { apiService } = await import('../../../services/api')
const { usePipelineActions } = await import('../hooks/usePipelineActions')
const { PipelineCardGrid } = await import('./PipelineCardGrid')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const pipelines = Array.from({ length: 8 }, (_, index) =>
  viewPipelineFixture({ id: `pipeline-${index}`, name: `Workflow ${index}` })
)
const provider = providerSummaryFixture()

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  resizeCallbacks.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

function Grid(): ReactNode {
  const { actions } = usePipelineActions({
    view: undefined,
    onTrigger: () => undefined,
    onViewHistory: () => undefined,
  })

  return createElement(PipelineCardGrid, { pipelines, getProvider: () => provider, actions })
}

function resizeTo(width: number) {
  containerWidth = width
  act(() => {
    for (const notify of resizeCallbacks) {
      notify()
    }
  })
}

function rowCount(container: HTMLElement): number {
  const list = container.querySelector<HTMLElement>('div[style*="relative"]')

  return Number.parseInt(list?.style.height ?? '0', 10) / CARD_ROW_HEIGHT
}

test('the card grid follows its own width, not the window, and keeps a row for every pipeline', async (context) => {
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  context.mock.method(apiService, 'getCachedPipelines', async () => [])
  containerWidth = 1200
  const { container } = render(
    createElement(
      MantineProvider,
      null,
      createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, createElement(Grid)))
    )
  )

  assert.equal(rowCount(container), 2)

  resizeTo(600)
  assert.equal(rowCount(container), 4)

  resizeTo(300)
  assert.equal(rowCount(container), pipelines.length)
})
