import type { RunHistoryQuery } from '../types/run_history'

export function runHistoryQuery(
  page: number,
  pageSize: number,
  overrides: Partial<RunHistoryQuery> = {}
): RunHistoryQuery {
  return {
    page,
    page_size: pageSize,
    statuses: [],
    utc_offset_minutes: -new Date().getTimezoneOffset(),
    ...overrides,
  }
}
