import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, Fragment, useRef } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider, Menu, createTheme } = await import('@mantine/core')
const { MemoryRouter, useNavigate } = await import('react-router-dom')
const { ContextMenuHost, useContextMenu } = await import('./ContextMenu')
const { SearchInput } = await import('./SearchInput')
const { useFormEnterNavigation } = await import('../../hooks/useFormEnterNavigation')

after(async () => {
  cleanup()
  await GlobalRegistrator.unregister()
})

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

function QuietTrigger({ entries }: { entries: Array<{ label: string; disabled?: boolean; onClick: () => void }> }) {
  const contextMenu = useContextMenu()

  return createElement('button', { type: 'button', ...contextMenu(() => entries) }, 'Quiet')
}

function renderQuiet(entries: Array<{ label: string; disabled?: boolean; onClick: () => void }>) {
  return render(
    createElement(
      MantineProvider,
      { theme: testTheme },
      createElement(MemoryRouter, null, createElement(QuietTrigger, { entries }), createElement(ContextMenuHost))
    )
  )
}

test('context menu stays closed and keeps the native menu when there are no entries', () => {
  renderQuiet([])
  assert.equal(fireEvent.contextMenu(view().getByRole('button', { name: 'Quiet' })), true)
  assert.equal(view().queryByRole('menu'), null)
})

test('context menu stays closed and keeps the native menu when every entry is disabled', () => {
  renderQuiet([{ label: 'Unavailable action', disabled: true, onClick: () => {} }])
  assert.equal(fireEvent.contextMenu(view().getByRole('button', { name: 'Quiet' })), true)
  assert.equal(view().queryByRole('menu'), null)
})

test('search loading exposes a named searching status while the textbox stays accessible', () => {
  render(
    createElement(
      MantineProvider,
      null,
      createElement(SearchInput, { value: 'build', onChange: () => {}, placeholder: 'Search runs', loading: true })
    )
  )
  assert.ok(view().getByRole('status', { name: 'Searching…' }))
  assert.ok(view().getByRole('textbox', { name: 'Search runs' }))
})

function EnterForm() {
  const formRef = useRef<HTMLFormElement>(null)

  useFormEnterNavigation(formRef)

  return createElement('form', { ref: formRef }, createElement('input', { 'aria-label': 'Name' }))
}

test('desktop Enter handling leaves native form submission untouched', () => {
  render(createElement(EnterForm))

  const event = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })

  view().getByRole('textbox', { name: 'Name' }).dispatchEvent(event)
  assert.equal(event.defaultPrevented, false)
})
