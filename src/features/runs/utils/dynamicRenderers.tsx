import { Anchor, Badge, Code, Text } from '@mantine/core'
import { StatusBadge } from '../../../components/atoms/StatusBadge'
import { RunDuration } from '../../../components/common/RunDuration'
import { STATUS_META } from '../../../theme/status'
import type { CellRenderer, PipelineRun } from '../../../types/domain'
import { formatDateTime } from '../../../utils/formatDateTime'
import { formatDuration } from '../../../utils/formatDuration'
import { isOneOf } from '../../../utils/isOneOf'
import { isRecord } from '../../../utils/isRecord'
import { isHttpUrl } from '../../../utils/openExternal'
import { TableCells } from '../../../utils/tableCells'
import { typedKeys } from '../../../utils/typedKeys'
import { useRunActions } from '../components/RunActionsContext'

export type CellValue = string | number | boolean | null | undefined | Record<string, unknown> | unknown[]

export function isCellValue(value: unknown): value is CellValue {
  return (
    value === null ||
    value === undefined ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean' ||
    Array.isArray(value) ||
    isRecord(value)
  )
}

const renderEmpty = (): React.ReactNode => TableCells.textDimmed('—')

const renderText = (value: CellValue): React.ReactNode => TableCells.text(String(value))

const renderBadge = (value: CellValue): React.ReactNode => (
  <Badge variant="light" color="gray" size="sm" fw={500}>
    {String(value)}
  </Badge>
)

const renderDateTime = (value: CellValue): React.ReactNode => {
  if (!value) {
    return renderEmpty()
  }

  const date = new Date(typeof value === 'number' ? value : String(value))

  return Number.isNaN(date.getTime()) ? renderText(value) : TableCells.text(formatDateTime(date))
}

const renderDuration = (value: CellValue): React.ReactNode => {
  const duration = typeof value === 'number' ? value : Number.parseInt(String(value), 10)

  return Number.isNaN(duration) ? renderEmpty() : TableCells.text(formatDuration(duration))
}

function LiveRunDurationCell({ run }: { run: PipelineRun }): React.ReactNode {
  const { pipeline } = useRunActions()

  return (
    <Text size="sm">
      <RunDuration run={run} typicalSeconds={pipeline?.typical_duration_seconds} />
    </Text>
  )
}

export const renderLiveRunDuration = (run: PipelineRun): React.ReactNode => <LiveRunDurationCell run={run} />

const renderLink = (value: CellValue): React.ReactNode => {
  const url = String(value)

  if (!isHttpUrl(url)) {
    return renderText(value)
  }

  return (
    <Anchor
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      size="sm"
      truncate="end"
      onClick={(event) => event.stopPropagation()}
    >
      {url}
    </Anchor>
  )
}

const renderJsonViewer = (value: CellValue): React.ReactNode => (
  <Code block fz="xs">
    {typeof value === 'string' ? value : JSON.stringify(value, null, 2)}
  </Code>
)

const RENDERER_MAP: Partial<Record<CellRenderer & string, (value: CellValue) => React.ReactNode>> = {
  Text: renderText,
  Badge: renderBadge,
  DateTime: renderDateTime,
  Duration: renderDuration,
  Commit: (value) => TableCells.commit(String(value)),
  Avatar: renderText,
  TruncatedText: (value) => TableCells.truncated(String(value)),
  Link: renderLink,
  JsonViewer: renderJsonViewer,
}

export const renderCellValue = (renderer: CellRenderer, value: CellValue, statusLabel?: string): React.ReactNode => {
  if (value === null || value === undefined) {
    return renderEmpty()
  }
  if (renderer === 'StatusBadge') {
    const status = String(value).toLowerCase()

    return isOneOf(typedKeys(STATUS_META), status) ? (
      <StatusBadge status={status} label={statusLabel} />
    ) : (
      renderText(value)
    )
  }

  const render = typeof renderer === 'string' ? RENDERER_MAP[renderer] : undefined

  return (render ?? renderText)(value)
}
