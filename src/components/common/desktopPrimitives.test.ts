import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider, Menu, createTheme } = await import('@mantine/core')
const { MemoryRouter, useNavigate } = await import('react-router-dom')
const { ContextMenuHost, useContextMenu } = await import('./ContextMenu')
const { SearchInput } = await import('./SearchInput')
const { handleFormEnter } = await import('../../utils/formFocus')

const view = () => within(document.body)

const testTheme = createTheme({
  components: { Menu: Menu.extend({ defaultProps: { transitionProps: { duration: 0 } } }) },
})
let selections = 0

function MenuTrigger() {
  const contextMenu = useContextMenu()
  const navigate = useNavigate()

  return createElement(
    Fragment,
    null,
    createElement(
      'button',
      {
        type: 'button',
        ...contextMenu(() => [
          { label: 'Unavailable action', disabled: true, onClick: () => {} },
          { divider: true },
          {
            label: 'Open run',
            onClick: () => {
              selections += 1
            },
          },
        ]),
      },
      'Run'
    ),
    createElement('button', { type: 'button', onClick: () => navigate('/settings') }, 'Settings')
  )
}

function renderMenu() {
  return render(
    createElement(
      MantineProvider,
      { theme: testTheme },
      createElement(MemoryRouter, null, createElement(MenuTrigger), createElement(ContextMenuHost))
    )
  )
}

beforeEach(() => {
  cleanup()
  selections = 0
})

test('search clears with Escape without closing the containing overlay', () => {
  const values: string[] = []
  const { rerender } = render(
    createElement(
      MantineProvider,
      null,
      createElement(SearchInput, {
        value: 'build',
        onChange: (value) => values.push(value),
        placeholder: 'Search runs',
      })
    )
  )
  const input = view().getByRole('textbox', { name: 'Search runs' })

  assert.ok(input.hasAttribute('data-search-input'))
  assert.equal(fireEvent.keyDown(input, { key: 'Escape' }), false)
  assert.deepEqual(values, [''])
  assert.ok(view().getByRole('button', { name: 'Clear search' }))

  rerender(
    createElement(
      MantineProvider,
      null,
      createElement(SearchInput, { value: '', onChange: (value) => values.push(value), placeholder: 'Search runs' })
    )
  )
  assert.equal(view().queryByRole('button', { name: 'Clear search' }), null)
})

test('context menu opens at the pointer, focuses the first enabled command and invokes it', async () => {
  renderMenu()
  fireEvent.contextMenu(view().getByRole('button', { name: 'Run' }), { clientX: 80, clientY: 100 })
  const command = await view().findByRole('menuitem', { name: 'Open run' })

  await waitFor(() => assert.equal(document.activeElement, command))
  fireEvent.click(command)
  assert.equal(selections, 1)
  await waitFor(() => assert.equal(view().queryByRole('menu'), null))
})

test('context menu opens from the keyboard', async () => {
  renderMenu()
  fireEvent.keyDown(view().getByRole('button', { name: 'Run' }), { key: 'F10', shiftKey: true })
  assert.ok(await view().findByRole('menu'))
})

test('context menu closes when the page scrolls', async () => {
  renderMenu()
  fireEvent.contextMenu(view().getByRole('button', { name: 'Run' }))
  assert.ok(await view().findByRole('menu'))
  fireEvent.scroll(window)
  await waitFor(() => assert.equal(view().queryByRole('menu'), null))
})

test('context menu closes when the route changes', async () => {
  renderMenu()
  fireEvent.contextMenu(view().getByRole('button', { name: 'Run' }))
  assert.ok(await view().findByRole('menu'))
  fireEvent.click(view().getByRole('button', { name: 'Settings' }))
  await waitFor(() => assert.equal(view().queryByRole('menu'), null))
})

test('desktop Enter handling leaves native form submission untouched', () => {
  const form = document.createElement('form')
  const input = document.createElement('input')
  let prevented = false

  form.append(input)
  handleFormEnter({
    key: 'Enter',
    defaultPrevented: false,
    nativeEvent: { isComposing: false },
    target: input,
    currentTarget: form,
    preventDefault: () => {
      prevented = true
    },
  } as unknown as Parameters<typeof handleFormEnter>[0])
  assert.equal(prevented, false)
})
