import { Text } from '@mantine/core'

import { StatusBadge as StatusBadgeComponent } from '../components/atoms/StatusBadge'
import type { CellRenderer, PipelineStatus } from '../types'

import { formatDuration } from './formatDuration'

export const THEME_COLORS = {
  // Field labels (Organization, Branch, etc.)
  FIELD_LABEL: 'gray.5',
  // Field values (main text content)
  VALUE_TEXT: 'dark.1',
  // Helper/dimmed text (descriptions, placeholders, loading messages)
  DIMMED: 'dimmed',
  // Modal/Section titles
  TITLE: 'gray.1',
  // Emphasized/Link text
  EMPHASIZED: 'blue',
} as const

export const THEME_TYPOGRAPHY = {
  // Modal/Section titles (large, bold)
  MODAL_TITLE: {
    size: 'lg' as const,
    weight: 600,
  },
  // Card/List item titles (repository names, workflow names, pipeline names)
  ITEM_TITLE: {
    size: 'sm' as const,
    weight: 500,
  },
  // Field labels (Organization, Branch, Duration, etc.)
  FIELD_LABEL: {
    size: 'xs' as const,
    weight: undefined,
  },
  // Field values (text content)
  FIELD_VALUE: {
    size: 'sm' as const,
    weight: undefined,
  },
  // Small field values (for compact layouts)
  FIELD_VALUE_SMALL: {
    size: 'xs' as const,
    weight: undefined,
  },
  // Helper/Description text
  HELPER_TEXT: {
    size: 'sm' as const,
    weight: undefined,
  },
} as const

/**
 * Dynamic cell renderers that map from schema CellRenderer enum to React components
 */
export class DynamicRenderers {
  static render(renderer: CellRenderer, value: any, isMobile = false): React.ReactNode {
    if (value === null || value === undefined) {
      return <Text size={isMobile ? 'sm' : 'md'} c="dimmed">—</Text>
    }

    // Handle enum variants
    if (typeof renderer === 'string') {
      switch (renderer) {
        case 'Text':
          return this.text(value)
        case 'Badge':
          return this.badge(value)
        case 'DateTime':
          return this.dateTime(value)
        case 'Duration':
          return this.duration(value)
        case 'StatusBadge':
          return this.statusBadge(value)
        case 'Commit':
          return this.commit(value)
        case 'Avatar':
          return this.avatar(value)
        case 'TruncatedText':
          return this.truncatedText(value)
        case 'Link':
          return this.link(value)
        case 'JsonViewer':
          return this.jsonViewer(value)
        default:
          return this.text(value)
      }
    }

    if (typeof renderer === 'object' && 'Custom' in renderer) {
      return this.text(value)
    }

    return this.text(value)
  }

  private static text(value: any): React.ReactNode {
    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
        {String(value)}
      </Text>
    )
  }

  private static badge(value: any): React.ReactNode {
    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
        {String(value)}
      </Text>
    )
  }

  private static dateTime(value: any): React.ReactNode {
    const date = new Date(value)

    if (isNaN(date.getTime())) {
      return <Text size="md" c={THEME_COLORS.VALUE_TEXT}>{String(value)}</Text>
    }

    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
        {date.toLocaleString()}
      </Text>
    )
  }

  private static duration(value: any): React.ReactNode {
    if (value === null || value === undefined) {
      return <Text size="md" c="dimmed">—</Text>
    }

    const duration = typeof value === 'number' ? value : Number.parseInt(String(value), 10)

    if (isNaN(duration)) {
      return <Text size="md" c="dimmed">—</Text>
    }

    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
        {formatDuration(duration)}
      </Text>
    )
  }

  private static statusBadge(value: any): React.ReactNode {
    const status = String(value).toLowerCase() as PipelineStatus

    return (
      <StatusBadgeComponent status={status} size="md" />
    )
  }

  private static commit(value: any): React.ReactNode {
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

  private static avatar(value: any): React.ReactNode {
    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT}>
        {String(value)}
      </Text>
    )
  }

  private static truncatedText(value: any): React.ReactNode {
    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT} truncate title={String(value)}>
        {String(value)}
      </Text>
    )
  }

  private static link(value: any): React.ReactNode {
    return (
      <Text
        size="md"
        c="blue"
        style={{ textDecoration: 'underline', cursor: 'pointer' }}
      >
        {String(value)}
      </Text>
    )
  }

  private static jsonViewer(value: any): React.ReactNode {
    const jsonStr = typeof value === 'string' ? value : JSON.stringify(value, null, 2)

    return (
      <Text size="md" c={THEME_COLORS.VALUE_TEXT} style={{ fontFamily: 'monospace' }}>
        {jsonStr}
      </Text>
    )
  }
}
