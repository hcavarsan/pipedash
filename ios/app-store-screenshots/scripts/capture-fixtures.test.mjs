import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { pipelineActions, runActions } from './capture-fixtures.mjs'

const actionTypes = readFileSync(new URL('../../../src/types/actions.ts', import.meta.url), 'utf8')

function interfaceFields(name) {
  const body = new RegExp(`export interface ${name} \\{([^}]*)\\}`).exec(actionTypes)?.[1]
  assert.ok(body, `${name} not found in src/types/actions.ts`)
  return [...body.matchAll(/^\s*(\w+):/gm)].map(([, field]) => field).sort()
}

test('the capture fixtures carry every pipeline and run action the app reads', () => {
  assert.deepEqual(Object.keys(pipelineActions).sort(), interfaceFields('PipelineActionState'))
  assert.deepEqual(Object.keys(runActions).sort(), interfaceFields('RunActionState'))
})
