import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 393, height: 852 })

const { cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MemoryRouter } = await import('react-router-dom')
const { IosRow } = await import('./IosList')

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

test('a row menu sits beside the row button, so pressing it never opens the row', async () => {
  const pressed: string[] = []

  render(
    <MemoryRouter>
      <IosRow
        title="Argo CD"
        chevron
        onClick={() => pressed.push('row')}
        menu={
          <button type="button" data-row-menu aria-label="Actions for Argo CD" onClick={() => pressed.push('menu')} />
        }
      />
    </MemoryRouter>
  )
  const view = within(document.body)
  const user = userEvent.setup({ document })
  const menu = view.getByRole('button', { name: 'Actions for Argo CD' })

  await user.click(menu)
  assert.deepEqual(pressed, ['menu'])
  assert.equal(menu.parentElement?.closest('button'), null)

  await user.click(view.getByRole('button', { name: 'Argo CD' }))
  assert.deepEqual(pressed, ['menu', 'row'])
})
