import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { SortableList } = await import('./SortableList')

const ids = ['a', 'b', 'c']
const ROW_HEIGHT = 40
const originalRect = Element.prototype.getBoundingClientRect

beforeEach(() => {
  Element.prototype.getBoundingClientRect = function rowRect(this: Element) {
    const index = ids.indexOf(this.getAttribute('data-row') ?? '')

    return index < 0 ? new DOMRect(0, 0, 0, 0) : new DOMRect(0, index * ROW_HEIGHT, 200, ROW_HEIGHT)
  }
})
afterEach(async () => {
  Element.prototype.getBoundingClientRect = originalRect
  await act(async () => cleanup())
})
after(async () => {
  await GlobalRegistrator.unregister()
})

function renderList(disabled: boolean | undefined, onReorder: (order: string[]) => void) {
  return render(
    <SortableList
      items={ids}
      getId={(id) => id}
      orientation="vertical"
      handle
      disabled={disabled}
      onReorder={onReorder}
    >
      {(id, drag) => (
        <div ref={drag.setNodeRef} style={drag.style} data-row={id}>
          <button type="button" {...drag.handleProps} {...drag.listeners} aria-label={`Reorder ${id}`}>
            {id}
          </button>
        </div>
      )}
    </SortableList>
  )
}

async function dragFirstRowDown() {
  const handle = within(document.body).getByRole('button', { name: 'Reorder a' })
  const pointer = { pointerId: 1, isPrimary: true, button: 0, clientX: 10 }

  await act(async () => {
    fireEvent.pointerDown(handle, { ...pointer, clientY: 20 })
  })
  await act(async () => {
    fireEvent.pointerMove(document, { ...pointer, clientY: 30 })
  })
  await act(async () => {
    fireEvent.pointerMove(document, { ...pointer, clientY: 75 })
  })
  await act(async () => {
    fireEvent.pointerUp(document, { ...pointer, clientY: 75 })
  })
}

test('dropping a row reports the new order', async () => {
  const onReorder = mock.fn()

  renderList(undefined, onReorder)
  await dragFirstRowDown()

  await waitFor(() => assert.equal(onReorder.mock.callCount(), 1))
  assert.deepEqual(onReorder.mock.calls[0]?.arguments[0], ['b', 'a', 'c'])
})

test('a disabled list cannot be dragged', async () => {
  const onReorder = mock.fn()

  renderList(true, onReorder)
  assert.equal(within(document.body).getByRole('button', { name: 'Reorder a' }).getAttribute('aria-disabled'), 'true')
  await dragFirstRowDown()

  assert.equal(onReorder.mock.callCount(), 0)
})
