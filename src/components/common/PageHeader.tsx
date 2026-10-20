import { ActionIcon, Box, Group, Stack, Text, Title } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'

interface PageHeaderProps {
  title: React.ReactNode
  subtitle?: string
  onBack?: () => void
  backLabel?: string
  actions?: React.ReactNode
}

export const PageHeader = ({ title, subtitle, onBack, backLabel = 'Back', actions }: PageHeaderProps) => {
  const { isMobile } = useIsMobile()

  return (
    <Box mih={isMobile ? 32 : 40}>
      <Group gap="xs" align="center" h={isMobile ? 32 : 40} justify="space-between" wrap="nowrap">
        <Group gap="xs" align="center" flex={1} miw={0}>
          {onBack && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size={isMobile ? 'sm' : 'md'}
              onClick={onBack}
              aria-label={backLabel}
            >
              <IconArrowLeft size={16} />
            </ActionIcon>
          )}
          <Stack gap={2} flex={1} miw={0}>
            <Title order={2} fw={600} size={isMobile ? 'h5' : 'h4'} lineClamp={1} miw={0}>
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
