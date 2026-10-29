import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 393, height: 852 })

const layout = {
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight'),
  offsetHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight'),
}
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.hasAttribute('data-virtual-item') ? 50 : 0
  },
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { VirtualList } = await import('./VirtualList')

const view = () => within(document.body)

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  for (const [name, descriptor] of Object.entries(layout)) {
    if (descriptor) {
      Object.defineProperty(HTMLElement.prototype, name, descriptor)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, name)
    }
  }
  await GlobalRegistrator.unregister()
})

function renderList(count: number, onEndReached: () => void) {
  const items = Array.from({ length: count }, (_, index) => `Item ${index + 1}`)

  return render(
    createElement(
      'div',
      { style: { overflowY: 'auto', height: '900px' } },
      createElement(VirtualList<string>, {
        items,
        getKey: (item) => item,
        estimateSize: 50,
        renderItem: (item) => createElement('p', null, item),
        onEndReached,
      })
    )
  )
}

test('a long list mounts only the visible window and does not ask for more', async () => {
  let ends = 0
  renderList(1000, () => {
    ends += 1
  })

  assert.ok(await view().findByText('Item 1'))
  assert.equal(view().queryByText('Item 1000'), null)
  assert.ok(document.querySelectorAll('[data-virtual-item]').length < 60)
  assert.equal(ends, 0)
})

test('reaching the end of a short list asks for the next page', async () => {
  let ends = 0
  renderList(10, () => {
    ends += 1
  })

  assert.ok(await view().findByText('Item 10'))
  await waitFor(() => assert.ok(ends > 0))
})
