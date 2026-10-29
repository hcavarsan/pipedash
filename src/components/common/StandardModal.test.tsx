import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { ReactNode } from 'react'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { sheetHandle } = await import('../../hooks/useSheetStack')
const { StandardModal } = await import('./StandardModal')

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  await act(async () => cleanup())
})

const view = () => within(document.body)

let closes = 0

const modal = (children: ReactNode): ReactNode => (
  <MantineProvider env="test">
    <StandardModal
      opened
      title="Sheet"
      onClose={() => {
        closes += 1
      }}
    >
      {children}
    </StandardModal>
  </MantineProvider>
)

function Broken(): ReactNode {
  throw new Error('body failed')
}

const openHandle = () => {
  const element = view().getByRole('dialog')
  const handle = sheetHandle(element)

  assert.ok(handle)

  return handle
}

test('a replaced closeNow resolves the first promise false', async () => {
  closes = 0
  render(modal('Body'))
  const handle = openHandle()
  const results: Array<boolean | undefined> = [undefined, undefined]

  await act(async () => {
    void handle.closeNow().then((closed) => {
      results[0] = closed
    })
  })
  await act(async () => {
    void handle.closeNow().then((closed) => {
      results[1] = closed
    })
  })

  assert.deepEqual(results, [false, undefined])
  assert.equal(closes, 2)
})

test('unmounting with a pending close resolves it false', async () => {
  closes = 0
  const result = render(modal('Body'))
  const handle = openHandle()
  let closed: boolean | undefined

  await act(async () => {
    void handle.closeNow().then((value) => {
      closed = value
    })
  })
  assert.equal(closed, undefined)
  await act(async () => {
    result.unmount()
  })
  assert.equal(closed, false)
})

test('a body that throws keeps the close control available', async () => {
  closes = 0
  const user = userEvent.setup({ document, delay: null })

  render(modal(<Broken />))
  await user.click(view().getByRole('button', { name: 'Done' }))

  await waitFor(() => assert.equal(closes, 1))
})
