import { ActionIcon, Badge, Box, Group, Stack, Text, Title } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'

import { useIsMobile } from '../../hooks/useIsMobile'

interface PageHeaderProps {
  title: React.ReactNode;
  subtitle?: string;
  badge?: string;
  onBack?: () => void;
  backLabel?: string;
  actions?: React.ReactNode;
}


export const PageHeader = ({
  title,
  subtitle,
  badge,
  onBack,
  backLabel = 'Back',
  actions,
}: PageHeaderProps) => {
  const { isMobile } = useIsMobile()

  return (
    <Box mb="xs" pt={0} pb={0} style={{ minHeight: isMobile ? 32 : 40 }}>
      <Group gap="xs" align="center" h={isMobile ? 32 : 40} justify="space-between" wrap="nowrap">
        <Group gap="xs" align="center" style={{ flex: 1, minWidth: 0 }}>
          {onBack && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size={isMobile ? 'sm' : 'md'}
              onClick={onBack}
              title={backLabel}
            >
              <IconArrowLeft size={isMobile ? 16 : 18} />
            </ActionIcon>
          )}
          <Stack gap={2} style={{ flex: 1, minWidth: 0 }}>
            <Group gap="xs" align="center" wrap="nowrap">
              <Title
                order={isMobile ? 5 : 3}
                fw={600}
                size={isMobile ? 'h6' : 'h4'}
                style={{
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  minWidth: 0,
                }}
              >
                {title}
              </Title>
              {badge && (
                <Badge
                  variant="light"
                  size={isMobile ? 'xs' : 'sm'}
                  fw={500}
                >
                  {badge}
                </Badge>
              )}
            </Group>
            {subtitle && (
              <Text size="xs" c="dimmed">
                {subtitle}
              </Text>
            )}
          </Stack>
        </Group>
        {actions && (
          <Box style={{ flexShrink: 0 }}>{actions}</Box>
        )}
      </Group>
    </Box>
  )
}
