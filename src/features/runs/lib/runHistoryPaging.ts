import type { RunHistoryPage } from '../../../types/run_history'

export interface RunHistoryPageParam {
  page: number
  scanDepth: number
}

export const FIRST_RUN_HISTORY_PAGE: RunHistoryPageParam = { page: 1, scanDepth: 1 }

export function nextRunHistoryPage(
  last: Pick<RunHistoryPage, 'has_more' | 'can_scan_deeper' | 'page_size'> & { runs: readonly unknown[] },
  { page, scanDepth }: RunHistoryPageParam
): RunHistoryPageParam | undefined {
  if (last.has_more) {
    return { page: page + 1, scanDepth }
  }
  if (!last.can_scan_deeper) {
    return undefined
  }
  if (last.runs.length < last.page_size) {
    return { page, scanDepth: scanDepth + 1 }
  }
  return { page: page + 1, scanDepth: scanDepth + 1 }
}
