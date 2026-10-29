import assert from 'node:assert/strict'
import { after, afterEach, mock, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 393, height: 852 })

const { act, cleanup, configure, render } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { useRowMenuLongPress } = await import('./useLongPress')

configure({ asyncWrapper: async (callback) => callback() })

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  mock.timers.reset()
  await act(async () => cleanup())
})

const opened = { count: 0 }
const defaultPrevented: boolean[] = []

document.addEventListener('keydown', (event) => {
  defaultPrevented.push(event.defaultPrevented)
})

function Row({ menu, nested }: { menu: boolean; nested?: boolean }) {
  const pressRef = useRowMenuLongPress()

  return createElement(
    'div',
    { ref: pressRef, role: 'group', 'aria-label': 'Row', tabIndex: 0 },
    'Row body',
    nested ? createElement('input', { 'aria-label': 'Nested field' }) : null,
    menu
      ? createElement('button', {
          type: 'button',
          'data-row-menu': '',
          'aria-label': 'Menu',
          onClick: () => {
            opened.count += 1
          },
        })
      : null
  )
}

const setup = () => {
  opened.count = 0
  defaultPrevented.length = 0

  return userEvent.setup({ document, delay: null })
}

test('releasing the pointer outside the row clears the press so the menu never opens', async () => {
  const user = setup()
  const result = render(createElement(Row, { menu: true }))
  const row = result.getByRole('group', { name: 'Row' })

  mock.timers.enable({ apis: ['setTimeout'] })
  await user.pointer([
    { keys: '[MouseLeft>]', target: row },
    { keys: '[/MouseLeft]', target: document.body },
  ])
  mock.timers.tick(700)

  assert.equal(opened.count, 0)
})

test('holding the pointer on the row still opens the menu', async () => {
  const user = setup()
  const result = render(createElement(Row, { menu: true }))
  const row = result.getByRole('group', { name: 'Row' })

  mock.timers.enable({ apis: ['setTimeout'] })
  await user.pointer({ keys: '[MouseLeft>]', target: row })
  mock.timers.tick(700)

  assert.equal(opened.count, 1)
  await user.pointer({ keys: '[/MouseLeft]', target: row })
})

test('the context-menu key keeps its default when the row has no enabled menu', async () => {
  const user = setup()
  const result = render(createElement(Row, { menu: false }))

  result.getByRole('group', { name: 'Row' }).focus()
  await user.keyboard('{ContextMenu}')

  assert.deepEqual(defaultPrevented, [false])
})

test('the context-menu key opens the menu and suppresses the browser menu when it succeeds', async () => {
  const user = setup()
  const result = render(createElement(Row, { menu: true }))

  result.getByRole('group', { name: 'Row' }).focus()
  await user.keyboard('{ContextMenu}')

  assert.equal(opened.count, 1)
  assert.deepEqual(defaultPrevented, [true])
})

test('the context-menu key inside a nested input keeps its default and does not open the menu', async () => {
  const user = setup()
  const result = render(createElement(Row, { menu: true, nested: true }))

  result.getByRole('textbox', { name: 'Nested field' }).focus()
  await user.keyboard('{ContextMenu}')

  assert.equal(opened.count, 0)
  assert.deepEqual(defaultPrevented, [false])
})
