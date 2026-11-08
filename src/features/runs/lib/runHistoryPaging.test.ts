import assert from 'node:assert/strict'
import { test } from 'node:test'

import { FIRST_RUN_HISTORY_PAGE, nextRunHistoryPage, type RunHistoryPageParam } from './runHistoryPaging'

const PAGE_SIZE = 10
const SCAN_BUDGET = 500

function serve(matches: number[], { page, scanDepth }: RunHistoryPageParam) {
  const scanned = SCAN_BUDGET * Math.max(page, scanDepth)
  const found = matches.filter((position) => position <= scanned)
  const start = (page - 1) * PAGE_SIZE
  return {
    runs: found.slice(start, start + PAGE_SIZE),
    page_size: PAGE_SIZE,
    has_more: start + PAGE_SIZE < found.length,
    can_scan_deeper: scanned < 1_200,
  }
}

function loadAll(matches: number[]): number[] {
  const seen = new Set<number>()
  let param: RunHistoryPageParam | undefined = FIRST_RUN_HISTORY_PAGE
  while (param) {
    const page = serve(matches, param)
    for (const run of page.runs) {
      seen.add(run)
    }
    param = nextRunHistoryPage(page, param)
  }
  return [...seen]
}

test('a match past the first scan budget is loaded on the same page instead of skipped', () => {
  assert.deepEqual(loadAll([501]), [501])
})

test('matches spread across budgets and pages all load', () => {
  const matches = [3, 501, ...Array.from({ length: 15 }, (_, index) => 1_001 + index)]
  assert.deepEqual(loadAll(matches), matches)
})

test('a full page with more matches advances the page and keeps the scan depth', () => {
  const page = {
    runs: Array.from({ length: PAGE_SIZE }, (_, index) => index),
    page_size: PAGE_SIZE,
    has_more: true,
    can_scan_deeper: false,
  }
  assert.deepEqual(nextRunHistoryPage(page, { page: 2, scanDepth: 3 }), { page: 3, scanDepth: 3 })
})
