import { Box, Group, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import type { SegmentKind, TimeSegment } from '../../../types/applications'

const SEGMENT_COLOR: Record<SegmentKind, string> = {
  build: 'var(--mantine-color-blue-filled)',
  wait: 'var(--mantine-color-gray-5)',
  deploy: 'var(--mantine-color-teal-filled)',
  build_to_deployed: 'var(--mantine-color-indigo-filled)',
}

export function TimeSegments({ segments }: { segments: TimeSegment[] }): ReactNode {
  if (segments.length === 0) {
    return null
  }

  return (
    <Box>
      <Group gap="4xs" wrap="nowrap" aria-hidden>
        {segments.map((segment) => (
          <Box
            key={segment.kind}
            data-segment={segment.kind}
            h={6}
            bdrs="xs"
            style={{ flexGrow: segment.seconds, flexBasis: 0, minWidth: 2, background: SEGMENT_COLOR[segment.kind] }}
          />
        ))}
      </Group>
      <Group gap="sm" mt="3xs">
        {segments.map((segment) => (
          <Group key={segment.kind} gap="3xs" wrap="nowrap">
            <Box w={8} h={8} bdrs="50%" style={{ background: SEGMENT_COLOR[segment.kind] }} aria-hidden />
            <Text size="xs" c="dimmed">
              {segment.label}
            </Text>
          </Group>
        ))}
      </Group>
    </Box>
  )
}
