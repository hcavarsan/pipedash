import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()

const observers: ObserverStub[] = []

class ObserverStub {
  disconnected = false
  constructor(readonly run: () => void) {
    observers.push(this)
  }
  observe() {}
  unobserve() {}
  disconnect() {
    this.disconnected = true
  }
}

Object.assign(window, { ResizeObserver: ObserverStub })
Object.assign(globalThis, { ResizeObserver: ObserverStub })

const { watchActions } = await import('./alertFit')

after(() => GlobalRegistrator.unregister())
afterEach(() => {
  observers.length = 0
  document.body.replaceChildren()
})

function actions(widths: Array<[number, number]>) {
  const group = document.createElement('div')

  for (const [scrollWidth, clientWidth] of widths) {
    const label = document.createElement('span')
    label.className = 'mantine-Button-label'
    Object.defineProperties(label, {
      scrollWidth: { configurable: true, get: () => widths[0]?.[0] ?? scrollWidth },
      clientWidth: { configurable: true, value: clientWidth },
    })
    group.append(label)
  }

  document.body.append(group)

  return group
}

test('buttons that fit stay in a row', () => {
  const group = actions([[50, 100]])

  watchActions(group)

  assert.equal(group.dataset.layout, 'row')
})

test('a resize that clips a label stacks the buttons and a later resize restores the row', () => {
  const widths: Array<[number, number]> = [[50, 100]]
  const group = actions(widths)
  watchActions(group)

  widths[0] = [150, 100]
  observers[0]?.run()
  assert.equal(group.dataset.layout, 'column')

  widths[0] = [50, 100]
  observers[0]?.run()
  assert.equal(group.dataset.layout, 'row')
})

test('the cleanup stops observing', () => {
  const group = actions([[50, 100]])

  watchActions(group)()

  assert.equal(observers[0]?.disconnected, true)
})
