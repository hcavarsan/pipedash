import type { LogSearchState } from '../hooks/useLogSearch'

export function searchCounter({ result, matches }: LogSearchState, currentIdx: number | null): string | null {
  if (!result) {
    return null
  }

  if (matches.length === 0) {
    return 'No results'
  }

  const position = currentIdx === null ? 0 : currentIdx + 1

  return `${position} of ${result.total}${result.capped ? '+' : ''}`
}
