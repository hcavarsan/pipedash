import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register()

const { act, cleanup, render } = await import('@testing-library/react')
const { MemoryRouter, useLocation } = await import('react-router-dom')
const { usePipelineFilters, useRunHistoryFilters } = await import('./useUrlState')

after(async () => {
  await act(async () => cleanup())
  GlobalRegistrator.unregister()
})

interface StatusFilters {
  setFilter: (key: 'dateRange', value: string | null) => void
  selectStatus: (status: string, selected: boolean) => void
  clearFilters: () => void
}

function mount(useFilters: () => StatusFilters, initial: string) {
  const seen: { filters: StatusFilters | null; search: string } = { filters: null, search: '' }

  function Probe() {
    seen.filters = useFilters()
    seen.search = useLocation().search

    return null
  }

  render(createElement(MemoryRouter, { initialEntries: [initial] }, createElement(Probe)))

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
    const filters = seen.filters as StatusFilters

    act(() => {
      filters.selectStatus('running', true)
      filters.selectStatus('pending', true)
      filters.setFilter('dateRange', '7d')
    })

    const params = new URLSearchParams(seen.search)
    assert.equal(params.get('status'), 'running,pending')
    assert.equal(params.get('dateRange'), '7d')
    assert.equal(params.get('search'), 'api')

    const next = seen.filters as StatusFilters

    act(() => {
      next.clearFilters()
      next.selectStatus('failed', true)
    })

    assert.equal(seen.search, '?status=failed')
  })

  test(`${name} status selection is idempotent, so a deselect reported twice still removes it once`, async () => {
    await act(async () => cleanup())
    const seen = mount(useFilters, '/pipelines?status=running,failed,pending')
    const filters = seen.filters as StatusFilters

    act(() => {
      filters.selectStatus('failed', true)
      filters.selectStatus('failed', false)
      filters.selectStatus('cancelled', false)
    })

    assert.equal(new URLSearchParams(seen.search).get('status'), 'running,pending')
  })
}
