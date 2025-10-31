import { Badge, Box, Button, Group, Stack, Text, Title } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'

interface PageHeaderProps {
  title: string;
  subtitle?: string;
  badge?: string;
  onBack?: () => void;
  backLabel?: string;
}


export const PageHeader = ({
  title,
  subtitle,
  badge,
  onBack,
  backLabel = 'Back',
}: PageHeaderProps) => {
  return (
    <Box mb="sm" pt={0} pb="xs">
      <Group justify="space-between" align="center">
        <Stack gap={2}>
          <Group gap="xs" align="center">
            <Title order={3} fw={600} size="h3">
              {title}
            </Title>
            {badge && (
              <Badge variant="light" size="md">
                {badge}
              </Badge>
            )}
          </Group>
          {subtitle && (
            <Text size="sm" c="dimmed">
              {subtitle}
            </Text>
          )}
        </Stack>
        {onBack && (
          <Button
            variant="subtle"
            size="sm"
            leftSection={<IconArrowLeft size={14} />}
            onClick={onBack}
          >
            {backLabel}
          </Button>
        )}
      </Group>
    </Box>
  )
}
