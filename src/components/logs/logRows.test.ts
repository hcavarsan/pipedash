import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { LogSection } from '../../types/logs'

import {
  buildRows,
  filterAfterJump,
  filterRows,
  isOpen,
  overridesFor,
  revealLine,
  rowAt,
  rowNearLine,
  rowOfLine,
  rowOfSection,
  sectionAtLine,
} from './logRows'

let nextId = 0

function section(partial: Partial<LogSection> & Pick<LogSection, 'kind' | 'start_line' | 'end_line'>): LogSection {
  return {
    id: nextId++,
    parent: null,
    node_id: null,
    title: `s${nextId}`,
    status: null,
    duration_ms: null,
    header_line: null,
    error_count: 0,
    open: false,
    ...partial,
  }
}

function fixture() {
  nextId = 0
  const build = section({ kind: 'step', title: 'Build', node_id: '1:1', start_line: 2, end_line: 12, open: true })
  const group = section({
    kind: 'group',
    title: 'Compile',
    parent: build.id,
    header_line: 4,
    start_line: 5,
    end_line: 9,
    open: false,
  })
  const test = section({ kind: 'step', title: 'Test', node_id: '1:2', start_line: 12, end_line: 15, open: false })
  return { build, group, test, meta: { line_count: 16, sections: [build, group, test] } }
}

test('closed sections hide their lines and open sections list them', () => {
  const { meta } = fixture()
  const model = buildRows(meta, new Set())

  assert.equal(model.count, 11)
  assert.deepEqual(rowAt(model, 0), { kind: 'line', line: 0 })
  assert.equal(rowAt(model, 2)?.kind, 'step')
  assert.deepEqual(rowAt(model, 4), { kind: 'line', line: 3 })
  assert.deepEqual(rowAt(model, 5), { kind: 'line', line: 4 })
  assert.deepEqual(rowAt(model, 6), { kind: 'line', line: 9 })
  assert.equal(rowAt(model, 9)?.kind, 'step')
  assert.equal(rowAt(model, model.count), null)
  assert.equal(rowAt(model, -1), null)
})

test('toggling a section through overrides flips its default', () => {
  const { meta, group, build } = fixture()
  const opened = buildRows(meta, new Set([group.id]))

  assert.equal(opened.count, 15)
  assert.deepEqual(rowAt(opened, 6), { kind: 'line', line: 5 })
  assert.equal(isOpen(build, new Set([build.id])), false)
  assert.equal(buildRows(meta, new Set([build.id])).count, 5)
})

test('rowOfLine is null for hidden lines and revealLine opens every ancestor', () => {
  const { meta, group } = fixture()
  const model = buildRows(meta, new Set())

  assert.equal(rowOfLine(model, 6), null)
  assert.equal(rowOfLine(model, 4), 5)
  assert.equal(rowOfLine(model, 99), null)

  const revealed = revealLine(meta, new Set(), 6)

  assert.deepEqual([...revealed], [group.id])
  assert.equal(rowOfLine(buildRows(meta, revealed), 6), 7)
  assert.equal(revealLine(meta, revealed, 6), revealed)
})

test('rowNearLine falls back to the closest visible row', () => {
  const { meta } = fixture()
  const model = buildRows(meta, new Set())

  assert.equal(rowNearLine(model, 6), rowOfLine(model, 4))
  assert.equal(rowNearLine(model, 0), 0)
  assert.equal(rowNearLine({ count: 0, segments: [] }, 5), 0)
})

test('rowOfSection finds step headers even when lines share their start', () => {
  const { meta, test: second, build } = fixture()
  const model = buildRows(meta, new Set())

  assert.equal(rowOfSection(model, build), 2)
  assert.equal(rowAt(model, rowOfSection(model, second) ?? -1)?.kind, 'step')
})

test('empty logs produce no rows and a step without lines still has a header', () => {
  nextId = 0
  assert.equal(buildRows({ line_count: 0, sections: [] }, new Set()).count, 0)

  const empty = section({ kind: 'step', start_line: 0, end_line: 0, open: true })

  assert.equal(buildRows({ line_count: 0, sections: [empty] }, new Set()).count, 1)
})

test('deeply nested groups do not overflow the stack', () => {
  nextId = 0
  const sections: LogSection[] = []

  for (let depth = 0; depth < 20_000; depth += 1) {
    sections.push(
      section({
        kind: 'group',
        parent: depth === 0 ? null : depth - 1,
        header_line: depth,
        start_line: depth + 1,
        end_line: 20_001,
        open: true,
      })
    )
  }

  assert.equal(buildRows({ line_count: 20_001, sections }, new Set()).count, 20_001)
})

test('filterRows merges runs and sorts and dedups lines', () => {
  const model = filterRows([9, 3, 4, 4, 5])

  assert.equal(model.count, 4)
  assert.equal(model.segments.length, 2)
  assert.deepEqual(rowAt(model, 0), { kind: 'line', line: 3 })
  assert.deepEqual(rowAt(model, 3), { kind: 'line', line: 9 })
  assert.equal(filterRows([]).count, 0)
})

test('sectionAtLine returns the step that contains the line', () => {
  const { meta, build, test: second } = fixture()

  assert.equal(sectionAtLine(meta, 1), null)
  assert.equal(sectionAtLine(meta, 2), build)
  assert.equal(sectionAtLine(meta, 11), build)
  assert.equal(sectionAtLine(meta, 12), second)
  assert.equal(sectionAtLine(meta, 15), null)
})

test('overridesFor keeps what the user chose across revisions', () => {
  nextId = 0
  const before = section({ kind: 'step', title: 'Build', node_id: 'a', start_line: 0, end_line: 5, open: false })
  const same = section({ kind: 'group', title: 'Run', start_line: 1, end_line: 3, header_line: 0, open: false })
  const prevMeta = { sections: [before, same] }

  nextId = 0
  const failed = section({ kind: 'step', title: 'Build', node_id: 'a', start_line: 0, end_line: 9, open: true })
  const sameAgain = section({ kind: 'group', title: 'Run', start_line: 1, end_line: 3, header_line: 0, open: false })
  const next = overridesFor({ sections: [failed, sameAgain] }, prevMeta, new Set([before.id, same.id]))

  assert.equal(isOpen(failed, next), true)
  assert.equal(isOpen(sameAgain, next), true)
  assert.equal(overridesFor({ sections: [failed] }, prevMeta, new Set()).size, 0)
})

test('a jump to a line outside the active filter clears it, and keeps it otherwise', () => {
  assert.equal(filterAfterJump('errors', 10, [3, 4], [10, 20]), 'errors')
  assert.equal(filterAfterJump('errors', 3, [3, 4], [10, 20]), 'none')
  assert.equal(filterAfterJump('matches', 3, [3, 4], [10, 20]), 'matches')
  assert.equal(filterAfterJump('matches', 10, [3, 4], [10, 20]), 'none')
  assert.equal(filterAfterJump('none', 10, [], []), 'none')
})

test('a cleared filter makes the target line reachable', () => {
  const { meta } = fixture()
  const filtered = filterRows([3, 4])

  assert.equal(rowOfLine(filtered, 8), null)
  assert.notEqual(rowOfLine(buildRows(meta, revealLine(meta, new Set(), 8)), 8), null)
})
