import { Button, Group, Stack, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { IosRow } from '../../../components/ios/IosList'

interface PipelinePreferenceRowProps {
  name: string
  detail: string
  action: string
  label: string
  mobileShell: boolean
  onAction: () => void
}

export function PipelinePreferenceRow({
  name,
  detail,
  action,
  label,
  mobileShell,
  onAction,
}: PipelinePreferenceRowProps): ReactNode {
  if (mobileShell) {
    return (
      <IosRow
        title={name}
        subtitle={detail}
        accessory={
          <Button variant="subtle" size="compact-sm" aria-label={label} onClick={onAction}>
            {action}
          </Button>
        }
      />
    )
  }

  return (
    <Group justify="space-between" wrap="nowrap" gap="sm">
      <Stack gap={0} miw={0}>
        <Text size="sm" truncate>
          {name}
        </Text>
        <Text size="xs" c="dimmed" truncate>
          {detail}
        </Text>
      </Stack>
      <Button variant="default" size="compact-sm" aria-label={label} onClick={onAction}>
        {action}
      </Button>
    </Group>
  )
}
