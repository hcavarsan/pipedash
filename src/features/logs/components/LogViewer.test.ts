import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { RefObject } from 'react'

import type { LogDocumentMeta, LogLinesPage, LogSearchQuery, LogSearchResult, RunLogNode } from '../../../types/logs'
import type { LogViewerHandle } from './LogViewer'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { createElement, createRef } = await import('react')
const { events } = await import('../../../services/service')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { LogViewer } = await import('./LogViewer')

const wasServer = environmentManager.isServer()

environmentManager.setIsServer(() => false)
notifyManager.setNotifyFunction((callback) => act(callback))

const view = () => within(document.body)

const node: RunLogNode = {
  id: '99',
  parent_id: null,
  kind: 'job',
  name: 'build',
  status: 'success',
  started_at: null,
  finished_at: null,
  duration_seconds: null,
  log: 'ready',
  web_url: null,
  depth: 0,
  navigable: true,
}

const meta: LogDocumentMeta = {
  node_id: '99',
  revision: 1,
  complete: true,
  truncated: false,
  line_count: 200,
  text_bytes: 2000,
  first_timestamp_ms: null,
  styles: [],
  sections: [],
  error_lines: [50, 120],
  warning_lines: [],
  error_total: 2,
  warning_total: 0,
  focus_line: null,
}

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

interface Layout {
  restore: () => void
}

function installLayout(): Layout {
  const saved = {
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth'),
    rect: Object.getOwnPropertyDescriptor(Element.prototype, 'getBoundingClientRect'),
    observer: Object.getOwnPropertyDescriptor(globalThis, 'ResizeObserver'),
  }

  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 400 })
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 800 })
  Object.defineProperty(Element.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect(0, 0, 100, 20),
  })
  Object.defineProperty(globalThis, 'ResizeObserver', {
    configurable: true,
    value: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  })

  const put = (target: object, name: string, descriptor: PropertyDescriptor | undefined) => {
    if (descriptor) {
      Object.defineProperty(target, name, descriptor)
    } else {
      Reflect.deleteProperty(target, name)
    }
  }

  return {
    restore: () => {
      put(HTMLElement.prototype, 'clientHeight', saved.clientHeight)
      put(HTMLElement.prototype, 'clientWidth', saved.clientWidth)
      put(Element.prototype, 'getBoundingClientRect', saved.rect)
      put(globalThis, 'ResizeObserver', saved.observer)
    },
  }
}

let layout: Layout | null = null

beforeEach(() => {
  client.clear()
  layout = installLayout()
})

afterEach(async () => {
  await act(async () => cleanup())
  layout?.restore()
  layout = null
})

after(async () => {
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  environmentManager.setIsServer(() => wasServer)
  await GlobalRegistrator.unregister()
})

type Search = (text: string) => LogSearchResult

function stubService(context: TestContext, search: Search) {
  const open = async () => ({ state: 'ready' as const, ...meta })
  const read = async (
    _p: string,
    _r: number,
    _n: string,
    _rev: number,
    start: number,
    count: number
  ): Promise<LogLinesPage> => ({
    start,
    lines: Array.from({ length: count }, (_, index) => ({ text: `line ${start + index}` })),
  })
  const find = async (
    _p: string,
    _r: number,
    _n: string,
    _rev: number,
    _s: string,
    query: LogSearchQuery
  ): Promise<LogSearchResult> => search(query.text)

  for (const target of [apiService, tauriService]) {
    context.mock.method(target, 'openRunLog', open)
    context.mock.method(target, 'readRunLogLines', read)
    context.mock.method(target, 'searchRunLog', find)
  }
  context.mock.method(events, 'listen', async () => () => undefined)
}

function mount(ref: RefObject<LogViewerHandle | null>) {
  return render(
    createElement(
      MemoryRouter,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(
          MantineProvider,
          null,
          createElement(LogViewer, {
            ref,
            pipelineId: 'github__1__o/r/w',
            pipelineName: 'pipe',
            runNumber: 7,
            node,
            steps: [],
            variant: 'desktop',
            providerName: 'GitHub',
          })
        )
      )
    )
  )
}

const matches = (...lines: number[]): LogSearchResult => ({
  matches: lines.map((line) => [line, 0, 4]),
  total: lines.length,
  capped: false,
})

const lineButton = (line: number) => view().queryByRole('button', { name: `Select line ${line + 1}` })

test('the imperative handle moves search, matches, errors and scroll', async (context) => {
  stubService(context, () => matches(10, 80, 150))
  const ref = createRef<LogViewerHandle>()
  const user = userEvent.setup({ document })

  mount(ref)
  await waitFor(() => assert.ok(lineButton(0)))
  const handle = ref.current

  assert.ok(handle)

  act(() => handle.focusSearch())
  assert.equal(document.activeElement, view().getByRole('textbox', { name: 'Search logs' }))

  act(() => handle.focusBody())
  assert.equal(document.activeElement, view().getByRole('region', { name: 'Log for build' }))

  act(() => handle.scrollToEnd())
  await waitFor(() => assert.ok(lineButton(199)))
  act(() => handle.scrollToStart())
  await waitFor(() => assert.ok(lineButton(0)))

  act(() => handle.nextError())
  await waitFor(() => assert.ok(lineButton(50)))
  act(() => handle.nextError())
  await waitFor(() => assert.ok(lineButton(120)))
  act(() => handle.prevError())
  await waitFor(() => assert.ok(lineButton(50)))

  act(() => handle.scrollToStart())
  await waitFor(() => assert.ok(lineButton(0)))
  await user.type(view().getByRole('textbox', { name: 'Search logs' }), 'line')
  await waitFor(() => assert.ok(view().getAllByText('1 of 3').length > 0), { timeout: 3000 })
  act(() => handle.nextMatch())
  await waitFor(() => assert.ok(view().getAllByText('2 of 3').length > 0))
  act(() => handle.prevMatch())
  await waitFor(() => assert.ok(view().getAllByText('1 of 3').length > 0))
})

test('a search changed during the debounce jumps to the new result, not the old one', async (context) => {
  stubService(context, (text) => (text === 'foo' ? matches(80) : matches(180)))
  const ref = createRef<LogViewerHandle>()
  const user = userEvent.setup({ document, delay: null })

  mount(ref)
  await waitFor(() => assert.ok(lineButton(0)))
  assert.equal(lineButton(80), null)

  const input = view().getByRole('textbox', { name: 'Search logs' })

  await user.type(input, 'foo')
  await waitFor(() => assert.ok(lineButton(80)), { timeout: 3000 })
  act(() => ref.current?.scrollToStart())
  await waitFor(() => assert.equal(lineButton(80), null))

  await user.type(input, 'b')
  await user.click(view().getByRole('button', { name: 'Log options' }))
  await user.click(await view().findByText('Show error lines only'))

  await waitFor(() => assert.ok(lineButton(180)), { timeout: 3000 })
})
