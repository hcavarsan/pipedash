import { Badge, Text } from '@mantine/core'

import { StatusBadge as StatusBadgeComponent } from '../components/atoms/StatusBadge'
import type { CellRenderer, PipelineStatus } from '../types'

import { formatDuration } from './formatDuration'

/**
 * Dynamic cell renderers that map from schema CellRenderer enum to React components
 */
export class DynamicRenderers {
  static render(renderer: CellRenderer, value: any): React.ReactNode {
    if (value === null || value === undefined) {
      return <Text size="sm" c="dimmed">—</Text>
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
      <Text size="sm" fw={600}>
        {String(value)}
      </Text>
    )
  }

  private static badge(value: any): React.ReactNode {
    return (
      <Badge variant="light" size="md">
        {String(value)}
      </Badge>
    )
  }

  private static dateTime(value: any): React.ReactNode {
    const date = new Date(value)

    if (isNaN(date.getTime())) {
      return <Text size="sm" c="dimmed">{String(value)}</Text>
    }

    return (
      <Text size="sm" c="dimmed">
        {date.toLocaleString()}
      </Text>
    )
  }

  private static duration(value: any): React.ReactNode {
    if (value === null || value === undefined) {
      return <Text size="sm" c="dimmed">—</Text>
    }

    const duration = typeof value === 'number' ? value : Number.parseInt(String(value), 10)

    if (isNaN(duration)) {
      return <Text size="sm" c="dimmed">—</Text>
    }

    return (
      <Text size="sm" fw={600}>
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
      <Text size="sm" c="dimmed" style={{ fontFamily: 'monospace' }}>
        {shortSha}
      </Text>
    )
  }

  private static avatar(value: any): React.ReactNode {
    return (
      <Badge variant="light" size="sm" radius="xl">
        {String(value).charAt(0).toUpperCase()}
      </Badge>
    )
  }

  private static truncatedText(value: any): React.ReactNode {
    return (
      <Text size="sm" c="dimmed" truncate title={String(value)}>
        {String(value)}
      </Text>
    )
  }

  private static link(value: any): React.ReactNode {
    return (
      <Text
        size="sm"
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
      <Text size="xs" c="dimmed" style={{ fontFamily: 'monospace' }}>
        {jsonStr}
      </Text>
    )
  }
}
