import { Alert, Badge, Button, Group, List, Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import type { ApplicationDetail } from '../../../types/applications'
import { CopyTomlButton } from './CopyTomlButton'
import { EditApplicationMenu } from './EditApplicationMenu'
import { TargetChips } from './TargetChips'

interface DetailHeaderActionsProps {
  detail: ApplicationDetail
  onOpenSuggestions: () => void
  onDeleted: () => void
}

export function DetailHeaderActions({ detail, onOpenSuggestions, onDeleted }: DetailHeaderActionsProps): ReactNode {
  const { summary, copy, suggestions } = detail

  return (
    <Group gap="xs">
      {suggestions.length > 0 && (
        <Button size="xs" variant="default" onClick={onOpenSuggestions}>
          {copy.suggestions}
          <Badge ml="xs" size="xs" variant="light">
            {suggestions.length}
          </Badge>
        </Button>
      )}
      <CopyTomlButton toml={detail.toml} copy={copy} />
      {summary.capabilities.edit && <EditApplicationMenu detail={detail} copy={copy} onDeleted={onDeleted} />}
    </Group>
  )
}

export function DetailNotices({ detail }: { detail: ApplicationDetail }): ReactNode {
  const { summary, copy } = detail

  return (
    <Stack gap="sm" mb="sm">
      {detail.config_defined && (
        <Alert variant="light" color="gray">
          {copy.config_notice}
        </Alert>
      )}
      {summary.warnings.length > 0 && (
        <Alert variant="light" color="yellow" title={copy.warnings}>
          <List size="sm" listStyleType="none">
            {summary.warnings.map((warning) => (
              <List.Item key={warning}>{warning}</List.Item>
            ))}
          </List>
        </Alert>
      )}
      <TargetChips chips={summary.chips} arrows={summary.arrows} />
    </Stack>
  )
}
