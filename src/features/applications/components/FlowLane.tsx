import { Box, Group, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { useIsMobile } from '../../../hooks/useIsMobile'
import type { ApplicationCopy, Flow, FlowLink, FlowNode } from '../../../types/applications'
import { flowSize, type LinkGeometry, linkGeometry, MIN_SCALE } from '../lib/flowLayout'
import { FlowNodeBox } from './FlowNodeBox'
import { LINK_BUTTON_SIZE, LinkPopover } from './LinkPopover'

interface FlowLaneProps {
  flow: Flow
  copy: ApplicationCopy
  applicationId: number
  canEdit: boolean
}

interface PlacedLink {
  link: FlowLink
  geometry: LinkGeometry
}

const SUGGESTED_DASH = '6 4'

function placeLinks(flow: Flow, vertical: boolean): PlacedLink[] {
  const nodes = new Map<string, FlowNode>(flow.nodes.map((node) => [node.id, node]))

  return flow.links.flatMap((link) => {
    const from = nodes.get(link.from)
    const to = nodes.get(link.to)

    return from && to ? [{ link, geometry: linkGeometry(from, to, vertical) }] : []
  })
}

function LegendLine({ dash, label }: { dash: string | undefined; label: string }): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap">
      <svg width={28} height={8} aria-hidden>
        <line x1={0} y1={4} x2={28} y2={4} stroke="currentColor" strokeWidth={2} strokeDasharray={dash} />
      </svg>
      <Text size="xs" c="dimmed">
        {label}
      </Text>
    </Group>
  )
}

export function FlowLane({ flow, copy, applicationId, canEdit }: FlowLaneProps): ReactNode {
  const { isMobile } = useIsMobile()

  if (flow.nodes.length === 0) {
    return (
      <Text c="dimmed" ta="center" py="xl" role="status">
        {copy.flow_empty}
      </Text>
    )
  }

  const { width, height } = flowSize(flow, isMobile)
  const links = placeLinks(flow, isMobile)
  const half = LINK_BUTTON_SIZE / 2

  return (
    <Stack gap="sm">
      <Box style={{ overflowX: 'auto' }}>
        <svg
          viewBox={`0 0 ${width} ${height}`}
          style={{ display: 'block', width: '100%', minWidth: width * MIN_SCALE, maxWidth: width, height: 'auto' }}
        >
          <title>{copy.tab_flow}</title>
          {links.map(({ link, geometry }) => (
            <path
              key={link.id}
              data-link-id={link.id}
              d={geometry.path}
              fill="none"
              stroke="var(--mantine-color-dimmed)"
              strokeWidth={2}
              strokeDasharray={link.confidence === 'suggested' ? SUGGESTED_DASH : undefined}
            />
          ))}
          {flow.nodes.map((node) => (
            <FlowNodeBox key={node.id} node={node} vertical={isMobile} />
          ))}
          {links.map(({ link, geometry }) => (
            <foreignObject
              key={link.id}
              x={geometry.mid.x - half}
              y={geometry.mid.y - half}
              width={LINK_BUTTON_SIZE}
              height={LINK_BUTTON_SIZE}
            >
              <LinkPopover link={link} copy={copy} applicationId={applicationId} canEdit={canEdit} />
            </foreignObject>
          ))}
        </svg>
      </Box>
      <Group gap="md">
        <LegendLine dash={undefined} label={copy.legend_exact} />
        <LegendLine dash={SUGGESTED_DASH} label={copy.legend_suggested} />
      </Group>
    </Stack>
  )
}
