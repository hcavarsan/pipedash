import { Anchor, Badge, Code } from '@mantine/core'

import { STATUS_META } from '../theme/status'
import type { CellRenderer, PipelineStatus } from '../types'

import { formatDuration } from './formatDuration'
import { TableCells } from './tableCells'

export type CellValue = string | number | boolean | null | undefined | Record<string, unknown> | unknown[]

const HTTP_URL = /^https?:\/\//i

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

  return Number.isNaN(date.getTime()) ? renderText(value) : TableCells.text(date.toLocaleString())
}

const renderDuration = (value: CellValue): React.ReactNode => {
  const duration = typeof value === 'number' ? value : Number.parseInt(String(value), 10)

  return Number.isNaN(duration) ? renderEmpty() : TableCells.text(formatDuration(duration))
}

const renderStatusBadge = (value: CellValue): React.ReactNode => {
  const status = String(value).toLowerCase()

  return status in STATUS_META ? TableCells.status(status as PipelineStatus) : renderText(value)
}

const renderLink = (value: CellValue): React.ReactNode => {
  const url = String(value)

  if (!HTTP_URL.test(url)) {
    return renderText(value)
  }

  return (
    <Anchor
      href={url}
      target="_blank"
      rel="noreferrer"
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
  StatusBadge: renderStatusBadge,
  Commit: (value) => TableCells.commit(String(value)),
  Avatar: renderText,
  TruncatedText: (value) => TableCells.truncated(String(value)),
  Link: renderLink,
  JsonViewer: renderJsonViewer,
}

export const renderCellValue = (renderer: CellRenderer, value: CellValue): React.ReactNode => {
  if (value === null || value === undefined) {
    return renderEmpty()
  }

  const render = typeof renderer === 'string' ? RENDERER_MAP[renderer] : undefined

  return (render ?? renderText)(value)
}
