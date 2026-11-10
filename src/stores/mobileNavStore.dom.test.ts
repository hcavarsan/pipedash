import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 390, height: 844 })

const { cleanup, renderHook } = await import('@testing-library/react')
const { RouteKeyContext, useMobileNavStore, useMobileTitle } = await import('./mobileNavStore')

const inRoute = (key: string) => (props: { children?: React.ReactNode }) =>
  createElement(RouteKeyContext, { value: key }, props.children)

afterEach(() => {
  cleanup()
  useMobileNavStore.setState({ titles: {} })
})
after(() => GlobalRegistrator.unregister())

test('the page title is published under its route key, updated, cleared on null and kept on unmount', () => {
  const initialProps: { title: string | null | undefined } = { title: 'Pipelines' }
  const { rerender, unmount } = renderHook(({ title }) => useMobileTitle(title), {
    initialProps,
    wrapper: inRoute('pipelines|/pipelines?view=a'),
  })
  assert.equal(useMobileNavStore.getState().titles['pipelines|/pipelines?view=a'], 'Pipelines')

  rerender({ title: 'Runs' })
  assert.equal(useMobileNavStore.getState().titles['pipelines|/pipelines?view=a'], 'Runs')

  rerender({ title: undefined })
  assert.equal(useMobileNavStore.getState().titles['pipelines|/pipelines?view=a'], undefined)

  rerender({ title: 'Settings' })
  unmount()
  assert.equal(useMobileNavStore.getState().titles['pipelines|/pipelines?view=a'], 'Settings')
})

test('titles of different routes do not overwrite each other', () => {
  renderHook(() => useMobileTitle('Pinned'), { wrapper: inRoute('pinned|/pipelines') })
  renderHook(() => useMobileTitle('Provider A'), { wrapper: inRoute('pipelines|/pipelines?provider=1') })

  const { titles } = useMobileNavStore.getState()

  assert.equal(titles['pinned|/pipelines'], 'Pinned')
  assert.equal(titles['pipelines|/pipelines?provider=1'], 'Provider A')
})
