import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register()

const { act, cleanup, render } = await import('@testing-library/react')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { useMetricsFilters, usePipelineFilters, useRunHistoryFilters } = await import('./useUrlState')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

interface StatusFilters {
  setFilter: (key: 'dateRange', value: string | null) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
}

interface Seen {
  filters: StatusFilters | null
  search: string
}

function Probe({ useFilters, seen }: { useFilters: () => StatusFilters; seen: Seen }): null {
  seen.filters = useFilters()
  seen.search = useLocation().search

  return null
}

function mount(useFilters: () => StatusFilters, initial: string): Seen {
  const seen: Seen = { filters: null, search: '' }

  render(createElement(MemoryRouter, { initialEntries: [initial] }, createElement(Probe, { useFilters, seen })))

  return seen
}

const hooks: [string, () => StatusFilters][] = [
  ['pipeline list', usePipelineFilters],
  ['run history', useRunHistoryFilters],
]

for (const [name, useFilters] of hooks) {
  test(`${name} keeps every filter change made before the next render`, async () => {
    await act(async () => cleanup())
    const seen = mount(useFilters, '/pipelines?search=api')
    const filters = seen.filters
    assert.ok(filters)

    act(() => {
      filters.selectStatus('running', true)
      filters.selectStatus('pending', true)
      filters.setFilter('dateRange', '7d')
    })

    const params = new URLSearchParams(seen.search)
    assert.equal(params.get('status'), 'running,pending')
    assert.equal(params.get('dateRange'), '7d')
    assert.equal(params.get('search'), 'api')

    const next = seen.filters
    assert.ok(next)

    act(() => {
      next.clearFilters()
      next.selectStatus('failed', true)
    })

    assert.equal(seen.search, '?status=failed')
  })

  test(`${name} status selection is idempotent, so a deselect reported twice still removes it once`, async () => {
    await act(async () => cleanup())
    const seen = mount(useFilters, '/pipelines?status=running,failed,pending')
    const filters = seen.filters
    assert.ok(filters)

    act(() => {
      filters.selectStatus('failed', true)
      filters.selectStatus('failed', false)
      filters.selectStatus('cancelled', false)
    })

    assert.equal(new URLSearchParams(seen.search).get('status'), 'running,pending')
  })
}

interface Result<T> {
  value: T | null
  search: string
}

function ResultProbe<T>({ useHook, seen }: { useHook: () => T; seen: Result<T> }): null {
  seen.value = useHook()
  seen.search = useLocation().search

  return null
}

function mountResult<T>(useHook: () => T, initial: string): Result<T> {
  const seen: Result<T> = { value: null, search: '' }

  render(createElement(MemoryRouter, { initialEntries: [initial] }, createElement(ResultProbe<T>, { useHook, seen })))

  return seen
}

test('metrics filters keep both changes made before the next render', async () => {
  await act(async () => cleanup())
  const seen = mountResult(useMetricsFilters, '/metrics')
  const hook = seen.value
  assert.ok(hook)

  act(() => {
    hook.setFilter('dateRange', '7d')
    hook.setFilter('period', 'daily')
  })

  const params = new URLSearchParams(seen.search)
  assert.equal(params.get('dateRange'), '7d')
  assert.equal(params.get('period'), 'daily')
  assert.equal(seen.value?.filters.dateRange, '7d')
})

test('metrics filters drop default values from the url', async () => {
  await act(async () => cleanup())
  const seen = mountResult(useMetricsFilters, '/metrics?dateRange=7d&period=daily')
  const hook = seen.value
  assert.ok(hook)

  act(() => {
    hook.setFilter('dateRange', '24h')
    hook.setFilter('period', 'hourly')
  })

  assert.equal(seen.search, '')
})

test('prototype property names are not accepted as pipeline statuses', async () => {
  await act(async () => cleanup())
  const seen = mountResult(usePipelineFilters, '/pipelines?status=constructor,running,toString')

  assert.deepEqual(seen.value?.filters.status, ['running'])
})

test('run history ignores page values that are not positive whole numbers', async () => {
  for (const page of ['-2', '1.5', '0', 'abc', 'Infinity', '1e400']) {
    await act(async () => cleanup())
    const seen = mountResult(useRunHistoryFilters, `/pipelines/1?page=${page}`)

    assert.equal(seen.value?.filters.page, 1, page)
  }

  await act(async () => cleanup())
  assert.equal(mountResult(useRunHistoryFilters, '/pipelines/1?page=3').value?.filters.page, 3)
})
