import assert from 'node:assert/strict'
import { test } from 'node:test'

import { registerSheet, sheetMark } from '../hooks/useSheetStack'
import { useModalStore } from './modalStore'

test('run details open over a trigger dialog only after that dialog agrees to close', async () => {
  let allow = false

  useModalStore.getState().openTriggerModal('p1')

  const unregister = registerSheet(sheetMark(), {
    requestClose: async () => allow,
    closeNow: async () => allow,
  })

  await useModalStore.getState().openLogsModal('p1', 3)
  assert.equal(useModalStore.getState().logsModal.open, false)
  assert.equal(useModalStore.getState().triggerModal.open, true)

  allow = true
  await useModalStore.getState().openLogsModal('p1', 3)
  unregister()

  assert.deepEqual(useModalStore.getState().logsModal, { open: true, pipelineId: 'p1', runNumber: 3 })
  assert.equal(useModalStore.getState().triggerModal.open, false)
})
