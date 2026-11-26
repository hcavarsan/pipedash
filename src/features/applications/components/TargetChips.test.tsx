import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { cleanup, render } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { TargetChips } = await import('./TargetChips')

afterEach(() => cleanup())
after(() => GlobalRegistrator.unregister())

const chip = (group_key: string, label: string) => ({
  group_key,
  label,
  commit_short: null,
  status: 'success' as const,
  health_label: null,
  behind_label: null,
  count: 1,
  kind: 'target' as const,
})

const chips = [chip('a', 'dev'), chip('b', 'staging'), chip('c', 'prod')]

function mount(arrows: { from: string; to: string }[]) {
  return render(
    <MantineProvider env="test">
      <TargetChips chips={chips} arrows={arrows} />
    </MantineProvider>
  )
}

test('an arrow is rendered only for entries in arrows', () => {
  const view = mount([{ from: 'a', to: 'b' }])

  assert.equal(view.queryAllByRole('img', { name: /→/ }).length, 1)
  assert.ok(view.getByRole('img', { name: 'dev → staging' }))
  assert.equal(view.queryByRole('img', { name: 'staging → prod' }), null)
})

test('no arrows are drawn when the backend sends none', () => {
  const view = mount([])

  assert.equal(view.queryAllByRole('img', { name: /→/ }).length, 0)
  assert.ok(view.getByText('prod'))
})
