import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 390, height: 844 })

const { cleanup, renderHook } = await import('@testing-library/react')
const { useMobileNavStore, useMobileTitle } = await import('./mobileNavStore')

afterEach(() => {
  cleanup()
  useMobileNavStore.setState({ title: null })
})
after(() => GlobalRegistrator.unregister())

test('the page title is published, updated and cleared on unmount', () => {
  const initialProps: { title: string | null | undefined } = { title: 'Pipelines' }
  const { rerender, unmount } = renderHook(({ title }) => useMobileTitle(title), { initialProps })
  assert.equal(useMobileNavStore.getState().title, 'Pipelines')

  rerender({ title: 'Runs' })
  assert.equal(useMobileNavStore.getState().title, 'Runs')

  rerender({ title: undefined })
  assert.equal(useMobileNavStore.getState().title, null)

  rerender({ title: 'Settings' })
  unmount()
  assert.equal(useMobileNavStore.getState().title, null)
})
