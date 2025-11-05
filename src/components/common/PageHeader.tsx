import { ActionIcon, Badge, Box, Group, Stack, Text, Title } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'

interface PageHeaderProps {
  title: string;
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
  return (
    <Box mb="xs" pt={0} pb={0} style={{ minHeight: 40 }}>
      <Group gap="xs" align="center" h={40} justify="space-between">
        <Group gap="xs" align="center" style={{ flex: 1 }}>
          {onBack && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size="md"
              onClick={onBack}
              title={backLabel}
            >
              <IconArrowLeft size={18} />
            </ActionIcon>
          )}
          <Stack gap={2} style={{ flex: 1 }}>
            <Group gap="xs" align="center">
              <Title order={3} fw={600} size="h4">
                {title}
              </Title>
              {badge && (
                <Badge variant="light" size="sm">
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
          <Box>{actions}</Box>
        )}
      </Group>
    </Box>
  )
}
