import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'

import { pipelineViewFixture, providerSummaryFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 393, height: 852 })

Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })
mockWindows('main')

const CHIP_STEP = 120
const layout = {
  offsetLeft: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetLeft'),
  offsetWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth'),
  clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth'),
}

Object.defineProperty(HTMLElement.prototype, 'offsetLeft', {
  configurable: true,
  get(this: HTMLElement) {
    return this.parentElement ? Array.from(this.parentElement.children).indexOf(this) * CHIP_STEP : 0
  },
})
Object.defineProperty(HTMLElement.prototype, 'offsetWidth', { configurable: true, get: () => 110 })
Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 300 })

const providers = Array.from({ length: 6 }, (_, index) =>
  providerSummaryFixture({ id: index + 1, name: `Provider ${index + 1}` })
)
const viewsGate = Promise.withResolvers<unknown>()

mockIPC(
  (command) => {
    switch (command) {
      case 'list_pipeline_views':
        return viewsGate.promise
      case 'list_providers':
        return providers
      default:
        return null
    }
  },
  { shouldMockEvents: true }
)

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { PipelineViewStrip } = await import('./PipelineViewStrip')

after(async () => {
  await act(async () => cleanup())
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, name)
    }
  }
  await GlobalRegistrator.unregister()
})

test('the selected provider chip scrolls into view when the view list loads after the page', async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

  render(
    <MantineProvider>
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/pipelines?provider=6']}>
          <PipelineViewStrip />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )

  const nav = document.querySelector('nav[aria-label="Pipeline views"]')
  assert.ok(nav)
  await waitFor(() => assert.ok(client.getQueryData(['providers', 'list'])))

  await act(async () => {
    viewsGate.resolve([
      pipelineViewFixture({ id: 'all', kind: 'all', name: 'All pipelines' }),
      pipelineViewFixture({ id: 'pinned', kind: 'pinned', name: 'Pinned' }),
    ])
  })

  const active = await waitFor(() => {
    const chip = nav.querySelector('[aria-current="page"]')
    assert.ok(chip instanceof HTMLElement)

    return chip
  })
  const scroller = active.parentElement
  assert.ok(scroller)
  await waitFor(() => assert.ok(scroller.scrollLeft > 0, `scrollLeft stayed ${scroller.scrollLeft}`))
})
