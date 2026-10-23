import assert from 'node:assert/strict'
import { beforeEach, test } from 'node:test'

import { useHealthDismissalStore } from './healthDismissalStore'

beforeEach(() => {
  useHealthDismissalStore.setState({ dismissed: [] })
})

test('a dismissal hides only its own episode', () => {
  const { dismiss } = useHealthDismissalStore.getState()

  dismiss('7:ep-1')
  dismiss('7:ep-1')

  assert.deepEqual(useHealthDismissalStore.getState().dismissed, ['7:ep-1'])
  assert.equal(useHealthDismissalStore.getState().dismissed.includes('7:ep-2'), false)
})

test('dismissals are cleared when their episode is no longer active', () => {
  const { dismiss, prune } = useHealthDismissalStore.getState()

  dismiss('7:ep-1')
  dismiss('8:ep-9')
  prune(['8:ep-9'])

  assert.deepEqual(useHealthDismissalStore.getState().dismissed, ['8:ep-9'])

  prune([])

  assert.deepEqual(useHealthDismissalStore.getState().dismissed, [])
})

test('a new episode with the same wording is shown again after the old one ended', () => {
  const { dismiss, prune } = useHealthDismissalStore.getState()

  dismiss('7:ep-1')
  prune([])

  assert.equal(useHealthDismissalStore.getState().dismissed.includes('7:ep-2'), false)
})
