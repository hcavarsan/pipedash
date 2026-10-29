import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()

const { trackTouches } = await import('./touchTracker')

after(() => GlobalRegistrator.unregister())

function setup() {
  const element = document.createElement('div')
  const captured = new Set<number>()
  const released: number[] = []

  Object.assign(element, {
    hasPointerCapture: (id: number) => captured.has(id),
    setPointerCapture: (id: number) => captured.add(id),
    releasePointerCapture: (id: number) => {
      captured.delete(id)
      released.push(id)
    },
  })
  document.body.append(element)

  const log: string[] = []
  const stop = trackTouches(element, {
    axis: 'x',
    begin: () => {
      log.push('begin')

      return {}
    },
    move: () => log.push('move'),
    end: () => log.push('end'),
    cancel: () => log.push('cancel'),
  })

  const pointer = (type: string, init: PointerEventInit) =>
    element.dispatchEvent(
      new PointerEvent(type, {
        pointerId: 1,
        button: 0,
        pointerType: 'mouse',
        bubbles: true,
        cancelable: true,
        ...init,
      })
    )

  return { element, log, stop, pointer, released }
}

test('disposing the tracker mid drag cancels the gesture and releases pointer capture', () => {
  const { element, log, stop, pointer, released } = setup()

  pointer('pointerdown', { clientX: 0, clientY: 0 })
  pointer('pointermove', { clientX: 40, clientY: 0 })
  assert.deepEqual(log, ['begin', 'move'])

  stop()

  assert.deepEqual(log, ['begin', 'move', 'cancel'])
  assert.deepEqual(released, [1])
  element.remove()
})

test('starting a new gesture cancels the active one instead of dropping it', () => {
  const { element, log, pointer } = setup()

  pointer('pointerdown', { clientX: 0, clientY: 0 })
  pointer('pointermove', { clientX: 40, clientY: 0 })
  pointer('pointerdown', { clientX: 10, clientY: 0, pointerId: 2 })

  assert.deepEqual(log, ['begin', 'move', 'cancel', 'begin'])
  element.remove()
})

test('a lost pointer capture cancels the active drag once', () => {
  const { element, log, pointer } = setup()

  pointer('pointerdown', { clientX: 0, clientY: 0 })
  pointer('pointermove', { clientX: 40, clientY: 0 })
  element.dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: 1 }))
  pointer('pointerup', { clientX: 60, clientY: 0 })

  assert.deepEqual(log, ['begin', 'move', 'cancel'])
  element.remove()
})

test('a finished drag ends normally and a disposed tracker ignores later input', () => {
  const { element, log, stop, pointer } = setup()

  pointer('pointerdown', { clientX: 0, clientY: 0 })
  pointer('pointermove', { clientX: 40, clientY: 0 })
  pointer('pointerup', { clientX: 60, clientY: 0 })
  stop()
  pointer('pointerdown', { clientX: 0, clientY: 0 })

  assert.deepEqual(log, ['begin', 'move', 'end'])
  element.remove()
})
