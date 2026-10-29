import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { SubmitButton } = await import('./SubmitButton')

const view = () => within(document.body)

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

function SaveForm({ onSubmit }: { onSubmit: () => Promise<void> }): ReactNode {
  return createElement('form', { action: onSubmit }, createElement(SubmitButton, null, 'Save'))
}

test('the submit button stays disabled while its form action runs and re-enables afterwards', async () => {
  const user = userEvent.setup({ document })
  const save = Promise.withResolvers<void>()
  let submissions = 0
  const onSubmit = async () => {
    submissions += 1
    await save.promise
  }

  render(createElement(MantineProvider, null, createElement(SaveForm, { onSubmit })))

  const button = view().getByRole('button', { name: 'Save' })
  await user.click(button)

  await waitFor(() => assert.ok(button.hasAttribute('disabled')))
  await user.click(button)
  assert.equal(submissions, 1)

  await act(async () => save.resolve())
  await waitFor(() => assert.equal(button.hasAttribute('disabled'), false))
})
