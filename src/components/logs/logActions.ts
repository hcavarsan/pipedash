import type { LogLinesPage } from '../../types/logs'
import { formatCount } from '../../utils/formatCount'

export const FULL_LINE_UNITS = 1_048_576
export const COPY_MAX_LINES = 50_000
export const COPY_MAX_UNITS = 4_194_304
export const COPY_PAGE_LINES = 2000

const FILE_NAME_MAX = 255
const EXTENSION = '.log'

export class CopyTooLargeError extends Error {
  constructor() {
    super('The selection is too large to copy. Download the log instead.')
  }
}

const slug = (value: string) => value.replace(/[^A-Za-z0-9._-]+/g, '-')

export function exportFileName(pipelineName: string, runNumber: number, nodeName: string): string {
  const stem = `${slug(pipelineName)}-run-${runNumber}-${slug(nodeName)}`

  return stem.slice(0, FILE_NAME_MAX - EXTENSION.length) + EXTENSION
}

export async function collectLines(
  from: number,
  to: number,
  fetchPage: (start: number, count: number) => Promise<LogLinesPage>
): Promise<string> {
  if (to - from > COPY_MAX_LINES) {
    throw new CopyTooLargeError()
  }

  const lines: string[] = []
  let units = 0
  let next = from

  while (next < to) {
    const page = await fetchPage(next, Math.min(COPY_PAGE_LINES, to - next))

    if (page.lines.length === 0) {
      break
    }

    for (const line of page.lines) {
      units += line.text.length + 1

      if (units > COPY_MAX_UNITS) {
        throw new CopyTooLargeError()
      }

      lines.push(line.text)
    }

    next += page.lines.length
  }

  return lines.join('\n')
}

export function newSessionId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16))

  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

// `errorLines` is sorted and may be capped below `errorTotal`, so positions past the cap read as the count.
export function errorCounter(errorLines: readonly number[], errorTotal: number, line: number | null): string {
  if (errorTotal === 0) {
    return 'No errors'
  }

  let low = 0
  let high = errorLines.length

  while (line !== null && low < high) {
    const mid = (low + high) >>> 1

    if (errorLines[mid] < line) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return line !== null && errorLines[low] === line
    ? `Error ${(low + 1).toLocaleString()} of ${errorTotal.toLocaleString()}`
    : formatCount(errorTotal, 'error')
}
