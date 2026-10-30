import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode, useState } from 'react'

import type { VirtualTableSort } from './VirtualTable'

GlobalRegistrator.register({ width: 1400, height: 900 })

const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { VirtualTable } = await import('./VirtualTable')

interface Row {
  id: string
  name: string
  locked: boolean
}

const ROWS: Row[] = [
  { id: 'a', name: 'Alpha', locked: false },
  { id: 'b', name: 'Beta', locked: true },
  { id: 'c', name: 'Gamma', locked: false },
]

const view = () => within(document.body)

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  if (clientHeight) {
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
  } else {
    Reflect.deleteProperty(HTMLElement.prototype, 'clientHeight')
  }
  await GlobalRegistrator.unregister()
})

interface HarnessProps {
  onRowClick: (row: Row) => void
  onSelect: (keys: Set<string>) => void
}

function Harness({ onRowClick, onSelect }: HarnessProps): ReactNode {
  const [sort, setSort] = useState<VirtualTableSort>({ columnId: 'name', direction: 'asc' })
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())

  return createElement(
    MantineProvider,
    null,
    createElement(VirtualTable<Row>, {
      records: ROWS,
      getKey: (row) => row.id,
      height: 400,
      rowHeight: 48,
      label: 'Rows',
      sort,
      onSortChange: setSort,
      onRowClick,
      selection: {
        selectedKeys: selected,
        isSelectable: (row) => !row.locked,
        onChange: (keys) => {
          setSelected(keys)
          onSelect(keys)
        },
        label: (row) => `Select ${row.name}`,
      },
      columns: [
        { id: 'name', title: 'Name', sortable: true, render: (row) => row.name },
        {
          id: 'actions',
          title: 'Actions',
          render: (row) => createElement('button', { type: 'button' }, `Open ${row.name}`),
        },
      ],
    })
  )
}

function renderHarness() {
  const clicks: string[] = []
  const selections: string[][] = []

  render(
    createElement(Harness, {
      onRowClick: (row) => clicks.push(row.id),
      onSelect: (keys) => selections.push([...keys].sort()),
    })
  )

  return { clicks, selections }
}

test('a sortable header toggles between ascending and descending', async () => {
  const user = userEvent.setup({ document })
  renderHarness()

  const header = view().getByRole('columnheader', { name: /Name/ })
  assert.equal(header.getAttribute('aria-sort'), 'ascending')

  await user.click(within(header).getByRole('button'))
  assert.equal(header.getAttribute('aria-sort'), 'descending')

  await user.click(within(header).getByRole('button'))
  assert.equal(header.getAttribute('aria-sort'), 'ascending')
})

test('clicking a row opens it, clicking a control inside the row does not', async () => {
  const user = userEvent.setup({ document })
  const { clicks } = renderHarness()

  await user.click(await view().findByRole('cell', { name: 'Alpha' }))
  assert.deepEqual(clicks, ['a'])

  await user.click(view().getByRole('button', { name: 'Open Gamma' }))
  assert.deepEqual(clicks, ['a'])
})

test('select all picks only selectable rows and clears on the second click', async () => {
  const user = userEvent.setup({ document })
  const { selections } = renderHarness()

  assert.equal(view().queryByRole('checkbox', { name: 'Select Beta' }), null)

  await user.click(view().getByRole('checkbox', { name: 'Select all' }))
  assert.deepEqual(selections.at(-1), ['a', 'c'])

  await user.click(view().getByRole('checkbox', { name: 'Select all' }))
  assert.deepEqual(selections.at(-1), [])
})

test('with more pages left, select all stays unchecked and hands the choice to the caller', async () => {
  const user = userEvent.setup({ document })
  const toggles: string[] = []
  let endReached = 0

  render(
    createElement(
      MantineProvider,
      null,
      createElement(VirtualTable<Row>, {
        records: ROWS,
        getKey: (row) => row.id,
        height: 400,
        rowHeight: 48,
        label: 'Rows',
        onEndReached: () => {
          endReached += 1
        },
        selection: {
          selectedKeys: new Set(['a', 'b', 'c']),
          isSelectable: () => true,
          onChange: () => toggles.push('change'),
          label: (row) => `Select ${row.name}`,
          hasMore: true,
          onToggleAll: () => toggles.push('all'),
        },
        columns: [{ id: 'name', title: 'Name', render: (row) => row.name }],
      })
    )
  )

  const selectAll = view().getByRole('checkbox', { name: 'Select all' })
  assert.ok(selectAll instanceof HTMLInputElement)
  assert.equal(selectAll.checked, false)

  await user.click(selectAll)
  assert.deepEqual(toggles, ['all'])
  assert.ok(endReached > 0)
})

test('rows taller than the estimate are measured so only the visible window renders', async () => {
  const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight')
  Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
    configurable: true,
    get(this: HTMLElement) {
      return this.hasAttribute('aria-rowindex') ? 100 : 0
    },
  })

  try {
    const records = Array.from({ length: 200 }, (_, index) => ({ id: `r${index}`, name: `Row ${index}` }))

    render(
      createElement(
        MantineProvider,
        null,
        createElement(VirtualTable<{ id: string; name: string }>, {
          records,
          getKey: (row) => row.id,
          height: 900,
          rowHeight: 48,
          label: 'Tall rows',
          columns: [{ id: 'name', title: 'Name', render: (row) => row.name }],
        })
      )
    )

    assert.ok(await view().findByRole('cell', { name: 'Row 0' }))
    await act(async () => undefined)

    const rendered = document.querySelectorAll('tbody tr[aria-rowindex]').length
    assert.ok(rendered < 25, `rendered ${rendered} rows for a 900px viewport of 100px rows`)
  } finally {
    if (offsetHeight) {
      Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight)
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, 'offsetHeight')
    }
  }
})
