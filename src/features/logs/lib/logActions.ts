import type { LogLinesPage } from '../../../types/logs'
import { formatCount } from '../../../utils/formatCount'
import { partitionPoint } from './binarySearch'
import type { LogFilter } from './logRows'

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

export class IncompleteLogCopyError extends Error {
  constructor() {
    super('The log ended before the selection was copied. Try again.')
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
      throw new IncompleteLogCopyError()
    }

    for (const line of page.lines) {
      if ((line.omitted ?? 0) > 0) {
        throw new CopyTooLargeError()
      }

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
export function errorCounter(
  errorLines: readonly number[],
  errorTotal: number,
  line: number | null,
  failed = false
): string {
  if (errorTotal === 0) {
    return failed ? 'Failed, no error lines' : 'No errors'
  }

  const low = line === null ? 0 : partitionPoint(errorLines.length, (index) => (errorLines[index] ?? Infinity) < line)

  return line !== null && errorLines[low] === line
    ? `Error ${(low + 1).toLocaleString()} of ${errorTotal.toLocaleString()}`
    : formatCount(errorTotal, 'error')
}

export interface LogOptionsState {
  wrap: boolean
  onWrapChange: (value: boolean) => void
  timestamps: boolean
  onTimestampsChange: (value: boolean) => void
  filter: LogFilter
  onFilterChange: (filter: LogFilter) => void
  errorTotal: number
  exporting: boolean
  onExport: () => void
}

export interface LogOptionItem {
  id: 'wrap' | 'timestamps' | 'errors' | 'export'
  label: string
  checked?: boolean
  onSelect: () => void
  disabled?: boolean
}

export function logOptionItems(state: LogOptionsState): LogOptionItem[] {
  const errorsOnly = state.filter === 'errors'

  return [
    { id: 'wrap', label: 'Wrap lines', checked: state.wrap, onSelect: () => state.onWrapChange(!state.wrap) },
    {
      id: 'timestamps',
      label: 'Show timestamps',
      checked: state.timestamps,
      onSelect: () => state.onTimestampsChange(!state.timestamps),
    },
    {
      id: 'errors',
      label: 'Show error lines only',
      checked: errorsOnly,
      disabled: state.errorTotal === 0 && !errorsOnly,
      onSelect: () => state.onFilterChange(errorsOnly ? 'none' : 'errors'),
    },
    {
      id: 'export',
      label: state.exporting ? 'Exporting log…' : 'Export log',
      disabled: state.exporting,
      onSelect: state.onExport,
    },
  ]
}
