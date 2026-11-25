import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { MemoryRouter } = await import('react-router-dom')
const { RunInputs } = await import('./RunInputs')

const INPUTS = [
  { label: 'environment', value: 'staging' },
  { label: 'dry_run', value: 'true' },
]

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

function renderInputs(inputs: typeof INPUTS, mobileShell: boolean) {
  return render(
    <MantineProvider env="test">
      <MemoryRouter>
        <RunInputs inputs={inputs} title="Inputs" mobileShell={mobileShell} isMobile={false} />
      </MemoryRouter>
    </MantineProvider>
  )
}

for (const mobileShell of [false, true]) {
  test(`the inputs a run started with are listed under their own heading (mobile shell: ${mobileShell})`, () => {
    renderInputs(INPUTS, mobileShell)
    const section = within(document.body).getByRole('region', { name: 'Inputs' })

    assert.ok(within(section).getByText('environment'))
    assert.ok(within(section).getByText('staging'))
    assert.ok(within(section).getByText('dry_run'))
    assert.ok(within(section).getByText('true'))
  })
}

test('a run without inputs shows no inputs section', () => {
  renderInputs([], false)

  assert.equal(within(document.body).queryByRole('region', { name: 'Inputs' }), null)
})
