import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactNode } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { ErrorBoundary } = await import('./ErrorBoundary')

const view = () => within(document.body)
const state = { broken: true }
const originalConsoleError = console.error

beforeEach(() => {
  state.broken = true
  console.error = () => undefined
})

afterEach(async () => {
  console.error = originalConsoleError
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function Widget(): ReactNode {
  if (state.broken) {
    throw new Error('Widget exploded')
  }

  return <p>Widget ready</p>
}

function Page({ resetKey }: { resetKey: string }): ReactNode {
  return (
    <MantineProvider>
      <p>Sibling content</p>
      <ErrorBoundary scope="feature" name="Widget" resetKeys={[resetKey]}>
        <Widget />
      </ErrorBoundary>
    </MantineProvider>
  )
}

test('a failing child shows the fallback while siblings keep rendering, and retry recovers', async () => {
  const user = userEvent.setup({ document })
  render(<Page resetKey="a" />)

  assert.ok(view().getByRole('heading', { level: 2, name: 'Widget failed to load' }))
  assert.ok(view().getByText('Sibling content'))

  state.broken = false
  await user.click(view().getByRole('button', { name: 'Try again' }))

  assert.ok(view().getByText('Widget ready'))
})

test('unchanged reset keys keep the fallback, changed reset keys render the children again', () => {
  const { rerender } = render(<Page resetKey="a" />)
  state.broken = false

  rerender(<Page resetKey="a" />)
  assert.ok(view().getByText('Widget failed to load'))

  rerender(<Page resetKey="b" />)
  assert.ok(view().getByText('Widget ready'))
})
