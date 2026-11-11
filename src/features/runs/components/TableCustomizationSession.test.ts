import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { TableCustomizationModal } = await import('./TableCustomizationModal')

const view = () => within(document.body)
const columns = [
  { accessor: 'branch', title: 'Branch' },
  { accessor: 'actor', title: 'Actor' },
  { accessor: 'actions', title: 'Actions' },
]

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('a rejected save hides its error once the columns change, without a new submit', async () => {
  const user = userEvent.setup({ document })
  let applies = 0

  render(
    createElement(
      MantineProvider,
      null,
      createElement(TableCustomizationModal, {
        opened: true,
        onClose: () => undefined,
        columns,
        visibleColumns: columns,
        onApply: async () => {
          applies += 1
          throw new Error('save failed')
        },
        sessionKey: 1,
      })
    )
  )

  await user.click(await view().findByRole('button', { name: 'Apply' }))
  await view().findByText(/save failed/)

  await user.click(view().getByRole('button', { name: 'Move Branch down' }))
  await waitFor(() => assert.equal(view().queryAllByText(/save failed/).length, 0))
  assert.equal(applies, 1)
})
