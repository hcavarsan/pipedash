import { Anchor, Badge, Box, Group, Text } from '@mantine/core'
import { IconAlertTriangle, IconEyeOff } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { StatusGlyph } from '../../../components/atoms/StatusBadge'
import type { ApplicationCopy, ApplicationSummary, BuildSummary } from '../../../types/applications'
import { TargetChips } from './TargetChips'

interface ApplicationPartProps {
  application: ApplicationSummary
  copy: ApplicationCopy
}

export function ApplicationName({ application, copy }: ApplicationPartProps): ReactNode {
  const warnings = application.warnings.join('\n')

  return (
    <Group gap="xs" wrap="nowrap" miw={0}>
      <Anchor component={Link} to={`/applications/${application.id}`} fw={600} size="sm" truncate>
        {application.name}
      </Anchor>
      {application.hidden && (
        <Badge variant="default" size="sm" leftSection={<IconEyeOff size={14} aria-hidden />}>
          {copy.hidden}
        </Badge>
      )}
      {application.warnings.length > 0 && (
        <Box
          component="span"
          role="img"
          tabIndex={0}
          aria-label={`${copy.warnings}: ${warnings}`}
          title={warnings}
          display="inline-flex"
          c="var(--status-degraded)"
        >
          <IconAlertTriangle size={16} aria-hidden />
        </Box>
      )}
    </Group>
  )
}

export function BuildLine({ build }: { build: BuildSummary }): ReactNode {
  return (
    <Group gap="xs" wrap="nowrap">
      <StatusGlyph status={build.status} />
      <Text size="xs" c="dimmed">
        {build.label}
      </Text>
    </Group>
  )
}

export function ApplicationRow({ application, copy }: ApplicationPartProps): ReactNode {
  return (
    <Group
      gap="md"
      px="md"
      py="sm"
      wrap="nowrap"
      align="center"
      style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
    >
      <Box w={240} flex="none" miw={0}>
        <ApplicationName application={application} copy={copy} />
      </Box>
      <Box flex={1} miw={0}>
        <TargetChips chips={application.chips} arrows={application.arrows} />
      </Box>
      {application.build && <BuildLine build={application.build} />}
    </Group>
  )
}
