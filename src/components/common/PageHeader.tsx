import { ActionIcon, Box, Group, Stack, Text, Title, Tooltip } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useMobileTitle } from '../../stores/mobileNavStore'

interface PageHeaderProps {
  title: React.ReactNode
  subtitle?: string
  onBack?: () => void
  backLabel?: string
  actions?: React.ReactNode
}

export const PageHeader = ({ title, subtitle, onBack, backLabel = 'Back', actions }: PageHeaderProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()

  useMobileTitle(mobileShell && typeof title === 'string' ? title : null)

  if (mobileShell) {
    return subtitle || actions ? (
      <Group gap="xs" justify="space-between" wrap="nowrap" px="xs">
        <Text size="xs" c="dimmed" flex={1} miw={0}>
          {subtitle}
        </Text>
        {actions}
      </Group>
    ) : null
  }

  return (
    <Box mih={isMobile ? 32 : 40}>
      <Group gap="xs" align="center" h={isMobile ? 32 : 40} justify="space-between" wrap="nowrap">
        <Group gap="xs" align="center" flex={1} miw={0}>
          {onBack && (
            <Tooltip label={backLabel}>
              <ActionIcon size={isMobile ? 'sm' : 'md'} onClick={onBack} aria-label={backLabel}>
                <IconArrowLeft size={16} />
              </ActionIcon>
            </Tooltip>
          )}
          <Stack gap="4xs" flex={1} miw={0}>
            <Title
              order={2}
              fw={600}
              size={isMobile ? 'h5' : 'h4'}
              lineClamp={1}
              miw={0}
              tabIndex={-1}
              id="page-heading"
              data-page-heading
            >
              {title}
            </Title>
            {subtitle && (
              <Text size="xs" c="dimmed">
                {subtitle}
              </Text>
            )}
          </Stack>
        </Group>
        {actions && <Box style={{ flexShrink: 0 }}>{actions}</Box>}
      </Group>
    </Box>
  )
}
