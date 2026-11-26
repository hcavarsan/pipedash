import { Box, Group, Text, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'

import { useModalStore } from '../../../stores/modalStore'
import type { CellState, ChangeCell } from '../../../types/applications'

const FILLED: Partial<Record<CellState, string>> = {
  success: 'var(--status-success)',
  failed: 'var(--status-failed)',
  running: 'var(--status-running)',
  live: 'var(--status-success)',
}

function CellDot({ state }: { state: CellState }): ReactNode {
  const fill = FILLED[state]

  return (
    <Box
      component="span"
      data-state={state}
      w={12}
      h={12}
      bdrs="50%"
      style={{
        flexShrink: 0,
        background: fill ?? 'transparent',
        border: fill ? 'none' : '1.5px solid var(--mantine-color-dimmed)',
        boxShadow: state === 'live' ? '0 0 0 2px var(--mantine-color-body), 0 0 0 4px var(--status-success)' : 'none',
      }}
    />
  )
}

export function ChangeCellButton({ cell }: { cell: ChangeCell }): ReactNode {
  const openLogsModal = useModalStore((s) => s.openLogsModal)
  const content = (
    <Group gap="2xs" wrap="nowrap">
      <CellDot state={cell.state} />
      {cell.label !== null && (
        <Text size="xs" c="dimmed">
          {cell.label}
        </Text>
      )}
    </Group>
  )

  if (cell.pipeline_id === null || cell.run_number === null) {
    return content
  }

  const { pipeline_id: pipelineId, run_number: runNumber } = cell

  return (
    <UnstyledButton
      className="interactive-surface"
      aria-label={cell.label ?? `#${runNumber}`}
      onClick={() => openLogsModal(pipelineId, runNumber)}
    >
      {content}
    </UnstyledButton>
  )
}
