import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { act, cleanup, renderHook } = await import('@testing-library/react')
const { usePipelineExpansion } = await import('./usePipelineExpansion')
const { usePipelineExpansionStore } = await import('../stores/pipelineExpansionStore')

beforeEach(() => {
  cleanup()
  usePipelineExpansionStore.setState({ expanded: {} })
})

const render = (view: string, filterKey = '', filtersActive = false) =>
  renderHook((props) => usePipelineExpansion(props.view, props.filterKey, props.filtersActive), {
    initialProps: { view, filterKey, filtersActive },
  })

test('a toggled section stays toggled for that view after remounting', () => {
  const first = render('release')

  act(() => first.result.current.toggle('repo:acme/app', false))
  first.unmount()

  const again = render('release')
  const other = render('all')

  assert.equal(again.result.current.isOpen('repo:acme/app', false), true)
  assert.equal(other.result.current.isOpen('repo:acme/app', false), false)
  assert.deepEqual(usePipelineExpansionStore.getState().expanded, { release: { 'repo:acme/app': true } })
})

test('while filtering everything opens and toggles are not saved', () => {
  usePipelineExpansionStore.setState({ expanded: { all: { 'repo:a': false, 'view:1': false } } })
  const { result, rerender } = render('all', 'search=a', true)

  assert.equal(result.current.isOpen('repo:a', false), true)
  assert.equal(result.current.isOpen('view:1', true), true)

  act(() => result.current.toggle('repo:a', false))
  assert.equal(result.current.isOpen('repo:a', false), false)
  assert.deepEqual(usePipelineExpansionStore.getState().expanded.all, { 'repo:a': false, 'view:1': false })

  rerender({ view: 'all', filterKey: 'search=ab', filtersActive: true })
  assert.equal(result.current.isOpen('repo:a', false), true)

  rerender({ view: 'all', filterKey: '', filtersActive: false })
  assert.equal(result.current.isOpen('repo:a', false), false)
})

test('expand and collapse all save every id at once', () => {
  const { result } = render('all')

  act(() => result.current.setAll(['repo:a', 'view:1'], true))
  assert.equal(result.current.isOpen('repo:a', false), true)

  act(() => result.current.setAll(['repo:a', 'view:1'], false))
  assert.equal(result.current.isOpen('repo:a', false), false)
  assert.equal(result.current.isOpen('view:1', true), false)
})
