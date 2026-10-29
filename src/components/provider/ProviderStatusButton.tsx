import { Group, Stack, Text, Tooltip, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'

import { useMobileShell } from '../../hooks/useIsMobile'
import { useNotificationCenterStore } from '../../stores/notificationCenterStore'
import type { ProviderSummary } from '../../types'
import { providerBadge } from '../../utils/fetchInfo'
import { providerAttention } from '../../utils/providerAttention'
import { TABLE_CELL_STYLES } from '../../utils/tableCells'
import { ProviderIcon } from '../atoms/ProviderIcon'

export function ProviderStatusButton({ provider }: { provider: ProviderSummary | undefined }): ReactNode {
  const open = useNotificationCenterStore((state) => state.open)
  const mobileShell = useMobileShell()

  if (!provider) {
    return (
      <Text size={TABLE_CELL_STYLES.text.size} c="dimmed">
        Unknown
      </Text>
    )
  }

  const { tone, headline } = providerAttention(provider)
  const attention = tone && headline ? { tone, label: headline } : undefined

  const button = (
    <UnstyledButton
      px={mobileShell ? 4 : '3xs'}
      py={mobileShell ? 2 : '4xs'}
      mih={mobileShell ? 44 : undefined}
      maw="100%"
      aria-label={`View ${provider.name} service status${attention ? `: ${headline}` : ''}`}
      style={{ borderRadius: 'var(--mantine-radius-sm)' }}
      onClick={(event) => {
        event.stopPropagation()
        open({ type: 'provider', providerId: provider.id })
      }}
    >
      <Group gap={TABLE_CELL_STYLES.spacing.gap} wrap="nowrap">
        <ProviderIcon name={provider.name} icon={provider.icon} size={22} badge={providerBadge(provider, Date.now())} />
        {mobileShell && attention ? (
          <Stack gap={0}>
            <Text size={TABLE_CELL_STYLES.text.size} truncate>
              {provider.name}
            </Text>
            <Text size="xs" c="dimmed" truncate>
              {headline}
            </Text>
          </Stack>
        ) : (
          <Text size={TABLE_CELL_STYLES.text.size} truncate>
            {provider.name}
          </Text>
        )}
      </Group>
    </UnstyledButton>
  )

  if (mobileShell) {
    return button
  }

  return (
    <Tooltip label={attention ? `${headline} · View service status` : 'View service status'} openDelay={300}>
      {button}
    </Tooltip>
  )
}
