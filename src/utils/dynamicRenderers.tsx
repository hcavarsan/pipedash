import { Text } from '@mantine/core'

import { StatusBadge as StatusBadgeComponent } from '../components/atoms/StatusBadge'
import type { CellRenderer, PipelineStatus } from '../types'

import { formatDuration } from './formatDuration'

export type CellValue =
  | string
  | number
  | boolean
  | null
  | undefined
  | Record<string, unknown>
  | unknown[]

export const THEME_COLORS = {
  FIELD_LABEL: 'gray.5',
  VALUE_TEXT: 'dark.1',
  DIMMED: 'dimmed',
  TITLE: 'gray.1',
  EMPHASIZED: 'blue',
} as const

export const THEME_TYPOGRAPHY = {
  MODAL_TITLE: {
    size: 'lg' as const,
    weight: 600,
  },
  ITEM_TITLE: {
    size: 'sm' as const,
    weight: 500,
  },
  FIELD_LABEL: {
    size: 'xs' as const,
    weight: undefined,
  },
  FIELD_VALUE: {
    size: 'sm' as const,
    weight: undefined,
  },
  FIELD_VALUE_SMALL: {
    size: 'xs' as const,
    weight: undefined,
  },
  HELPER_TEXT: {
    size: 'sm' as const,
    weight: undefined,
  },
} as const


const renderText = (value: CellValue): React.ReactNode => (
  <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
    {String(value)}
  </Text>
)

const renderBadge = (value: CellValue): React.ReactNode => (
  <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
    {String(value)}
  </Text>
)

const renderDateTime = (value: CellValue): React.ReactNode => {
  if (!value) {
    return <Text size="md" c="dimmed">—</Text>
  }

  const dateValue = typeof value === 'string' || typeof value === 'number'
    ? value
    : value instanceof Date
    ? value
    : String(value)

  const date = new Date(dateValue)

  if (Number.isNaN(date.getTime())) {
    return <Text size="md" c={THEME_COLORS.VALUE_TEXT}>{String(value)}</Text>
  }

  return (
    <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
      {date.toLocaleString()}
    </Text>
  )
}

const renderDuration = (value: CellValue): React.ReactNode => {
  if (value === null || value === undefined) {
    return <Text size="md" c="dimmed">—</Text>
  }

  const duration = typeof value === 'number' ? value : Number.parseInt(String(value), 10)

  if (Number.isNaN(duration)) {
    return <Text size="md" c="dimmed">—</Text>
  }

  return (
    <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
      {formatDuration(duration)}
    </Text>
  )
}

const renderStatusBadge = (value: CellValue): React.ReactNode => {
  const status = String(value).toLowerCase() as PipelineStatus


  
return <StatusBadgeComponent status={status} size="md" />
}

const renderCommit = (value: CellValue): React.ReactNode => {
  const sha = String(value)
  const shortSha = sha.substring(0, 7)

  return (
    <Text
      size="md"
      c={THEME_COLORS.VALUE_TEXT}
      style={{
        fontFamily: 'monospace',
        backgroundColor: 'var(--mantine-color-dark-7)',
        padding: '4px 10px',
        borderRadius: '6px',
        border: '1px solid var(--mantine-color-dark-5)',
        display: 'inline-block',
      }}
    >
      {shortSha}
    </Text>
  )
}

const renderAvatar = (value: CellValue): React.ReactNode => (
  <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
    {String(value)}
  </Text>
)

const renderTruncatedText = (value: CellValue): React.ReactNode => (
  <Text size="md" c={THEME_COLORS.VALUE_TEXT} truncate title={String(value)}>
    {String(value)}
  </Text>
)

const renderLink = (value: CellValue): React.ReactNode => (
  <Text
    size="md"
    c="blue"
    style={{ textDecoration: 'underline', cursor: 'pointer' }}
  >
    {String(value)}
  </Text>
)

const renderJsonViewer = (value: CellValue): React.ReactNode => {
  const jsonStr = typeof value === 'string' ? value : JSON.stringify(value, null, 2)

  return (
    <Text size="md" c={THEME_COLORS.VALUE_TEXT} style={{ fontFamily: 'monospace' }}>
      {jsonStr}
    </Text>
  )
}

const renderEmpty = (isMobile: boolean): React.ReactNode => (
  <Text size={isMobile ? 'sm' : 'md'} c="dimmed">—</Text>
)


type RendererFn = (value: CellValue) => React.ReactNode

const RENDERER_MAP: Record<string, RendererFn> = {
  Text: renderText,
  Badge: renderBadge,
  DateTime: renderDateTime,
  Duration: renderDuration,
  StatusBadge: renderStatusBadge,
  Commit: renderCommit,
  Avatar: renderAvatar,
  TruncatedText: renderTruncatedText,
  Link: renderLink,
  JsonViewer: renderJsonViewer,
}


export const renderCellValue = (
  renderer: CellRenderer,
  value: CellValue,
  isMobile = false
): React.ReactNode => {
  if (value === null || value === undefined) {
    return renderEmpty(isMobile)
  }

  if (typeof renderer === 'string') {
    const rendererFn = RENDERER_MAP[renderer]


    
return rendererFn ? rendererFn(value) : renderText(value)
  }

  if (typeof renderer === 'object' && 'Custom' in renderer) {
    return renderText(value)
  }

  return renderText(value)
}


export class DynamicRenderers {
  static render(renderer: CellRenderer, value: CellValue, isMobile = false): React.ReactNode {
    return renderCellValue(renderer, value, isMobile)
  }
}
