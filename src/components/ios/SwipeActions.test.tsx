import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 390, height: 844 })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { SwipeActions } = await import('./SwipeActions')

afterEach(async () => {
  await act(async () => cleanup())
  mock.restoreAll()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

function fullSwipe(label: string) {
  const slide = within(document.body).getByText(label).parentElement

  assert.ok(slide)
  Object.defineProperty(slide, 'offsetWidth', { configurable: true, value: 400 })
  slide.setPointerCapture = () => undefined
  slide.hasPointerCapture = () => false
  slide.releasePointerCapture = () => undefined

  const pointer = { pointerId: 1, pointerType: 'mouse', button: 0, clientY: 10 }

  fireEvent.pointerDown(slide, { ...pointer, clientX: 350 })
  fireEvent.pointerMove(slide, { ...pointer, clientX: 50 })
  fireEvent.pointerUp(slide, { ...pointer, clientX: 50 })
}

test('a disabled primary action does not run on a full swipe while an enabled one does', async () => {
  const enabled = mock.fn()
  const disabled = mock.fn()

  render(
    <>
      <SwipeActions actions={[{ key: 'go', label: 'Go', tone: 'blue', primary: true, onSelect: enabled }]}>
        <span>Enabled row</span>
      </SwipeActions>
      <SwipeActions
        actions={[{ key: 'wait', label: 'Wait', tone: 'blue', primary: true, disabled: true, onSelect: disabled }]}
      >
        <span>Disabled row</span>
      </SwipeActions>
    </>
  )

  fullSwipe('Disabled row')
  fullSwipe('Enabled row')

  await waitFor(() => assert.equal(enabled.mock.callCount(), 1), { timeout: 5000 })
  assert.equal(disabled.mock.callCount(), 0)
})

test('disabled swipe buttons cannot be activated', () => {
  const pin = mock.fn()
  const go = mock.fn()

  render(
    <SwipeActions
      actions={[
        { key: 'pin', label: 'Pin', tone: 'orange', disabled: true, onSelect: pin },
        { key: 'go', label: 'Go', tone: 'blue', primary: true, disabled: true, onSelect: go },
      ]}
    >
      <span>Row</span>
    </SwipeActions>
  )

  for (const name of ['Pin', 'Go']) {
    const button = within(document.body).getByText(name).closest('button')

    assert.ok(button)
    assert.equal(button.disabled, true)
    fireEvent.click(button)
  }

  assert.equal(pin.mock.callCount(), 0)
  assert.equal(go.mock.callCount(), 0)
})
