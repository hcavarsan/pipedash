import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

if (process.env.PIPEDASH_COLUMNS_TEST !== '1') {
  test('column customization behavior runs in its own DOM process', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_COLUMNS_TEST: '1' },
      encoding: 'utf8',
      timeout: 60000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
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
    await act(async () => cleanup())
    await GlobalRegistrator.unregister()
  })

  function renderModal(
    onApply: (order: string[], visibility: Record<string, boolean>) => Promise<void>,
    onClose: () => void
  ) {
    return render(
      createElement(
        MantineProvider,
        null,
        createElement(TableCustomizationModal, {
          opened: true,
          onClose,
          columns,
          visibleColumns: columns,
          onApply,
          sessionKey: 1,
        })
      )
    )
  }

  test('the last movable column cannot move below the fixed actions column', async () => {
    renderModal(
      async () => undefined,
      () => undefined
    )

    const down = await view().findByRole('button', { name: 'Move Actor down' })
    assert.ok(down instanceof HTMLButtonElement)
    assert.equal(down.disabled, true)
  })

  test('a failed save keeps the dialog open and a successful save closes it', async () => {
    const user = userEvent.setup({ document })
    let closed = 0
    let fail = true
    renderModal(
      async () => {
        if (fail) {
          throw new Error('save failed')
        }
      },
      () => {
        closed += 1
      }
    )

    await user.click(await view().findByRole('button', { name: 'Apply' }))
    await waitFor(() => assert.ok(view().queryByRole('alert')))
    assert.equal(closed, 0)

    fail = false
    await user.click(view().getByRole('button', { name: 'Apply' }))
    await waitFor(() => assert.equal(closed, 1))
  })
}
