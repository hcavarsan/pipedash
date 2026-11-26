import assert from 'node:assert/strict'
import { test } from 'node:test'

import { type DragItem, type DropZone, dropAction } from './boardDrop'

const pipeline = (applicationId: number | null, stage: 'build' | 'deploy' | null): DragItem => ({
  kind: 'pipeline',
  label: 'build',
  pipelineId: 'p1',
  applicationId,
  stage,
})

const target: DragItem = { kind: 'target', label: 'stage', targetKey: 't2', applicationId: 7, groupKey: 't2' }

test('an unassigned pipeline dropped on a lane links it with that stage', () => {
  assert.deepEqual(dropAction(pipeline(null, null), { kind: 'lane', applicationId: 7, stage: 'build' }), {
    kind: 'edit',
    applicationId: 7,
    edit: { type: 'link', pipeline_id: 'p1', stage: 'build' },
  })
})

test('a member dropped on the other lane of its application changes only the stage', () => {
  assert.deepEqual(dropAction(pipeline(7, 'build'), { kind: 'lane', applicationId: 7, stage: 'deploy' }), {
    kind: 'edit',
    applicationId: 7,
    edit: { type: 'stage', pipeline_id: 'p1', stage: 'deploy' },
  })
  assert.equal(dropAction(pipeline(7, 'build'), { kind: 'lane', applicationId: 7, stage: 'build' }), null)
})

test('a member dropped on another application links it there', () => {
  assert.deepEqual(dropAction(pipeline(7, 'deploy'), { kind: 'lane', applicationId: 9, stage: 'deploy' }), {
    kind: 'edit',
    applicationId: 9,
    edit: { type: 'link', pipeline_id: 'p1', stage: 'deploy' },
  })
})

test('a member dropped on the unassigned column is unlinked from its application', () => {
  assert.deepEqual(dropAction(pipeline(7, 'build'), { kind: 'unassigned' }), {
    kind: 'edit',
    applicationId: 7,
    edit: { type: 'unlink', pipeline_id: 'p1' },
  })
  assert.equal(dropAction(pipeline(null, null), { kind: 'unassigned' }), null)
})

test('a pipeline dropped on the new application zone asks to create one', () => {
  assert.deepEqual(dropAction(pipeline(null, null), { kind: 'new_application' }), {
    kind: 'create_application',
    pipelineId: 'p1',
  })
})

test('a target dropped on another environment joins every target under its label', () => {
  const zone: DropZone = {
    kind: 'environment',
    applicationId: 7,
    groupKey: 't1',
    label: 'prod',
    targetKeys: ['t1'],
  }

  assert.deepEqual(dropAction(target, zone), {
    kind: 'edit',
    applicationId: 7,
    edit: { type: 'target_group', target_keys: ['t2', 't1'], group_name: 'prod' },
  })
  assert.equal(dropAction(target, { ...zone, groupKey: 't2' }), null)
  assert.equal(dropAction(target, { ...zone, applicationId: 9 }), null)
})

test('a target only creates an environment inside its own application', () => {
  assert.deepEqual(dropAction(target, { kind: 'new_environment', applicationId: 7 }), {
    kind: 'create_environment',
    applicationId: 7,
    targetKey: 't2',
  })
  assert.equal(dropAction(target, { kind: 'new_environment', applicationId: 9 }), null)
  assert.equal(dropAction(target, { kind: 'lane', applicationId: 7, stage: 'build' }), null)
  assert.equal(dropAction(pipeline(null, null), { kind: 'new_environment', applicationId: 7 }), null)
})
