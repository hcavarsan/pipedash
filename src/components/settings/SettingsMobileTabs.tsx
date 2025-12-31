import { useState } from 'react'

import { Box, ScrollArea, Stack, Text, ThemeIcon, UnstyledButton } from '@mantine/core'
import { modals } from '@mantine/modals'
import { notifications } from '@mantine/notifications'
import {
  IconAlertTriangle,
  IconChartBar,
  IconDatabase,
  IconFolder,
  IconPlug,
  IconSettings,
  IconTrash,
} from '@tabler/icons-react'

import { service } from '../../services'

import type { SettingsSection } from './SettingsSidebar'

interface SettingsMobileTabsProps {
  activeSection: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  onRefresh?: () => Promise<void>;
}

const TABS: Array<{
  id: SettingsSection;
  label: string;
  icon: React.ReactNode;
}> = [
  { id: 'general', label: 'General', icon: <IconSettings size={18} /> },
  { id: 'providers', label: 'Providers', icon: <IconPlug size={18} /> },
  { id: 'metrics', label: 'Metrics', icon: <IconChartBar size={18} /> },
  { id: 'cache', label: 'Cache', icon: <IconDatabase size={18} /> },
  { id: 'storage', label: 'Storage', icon: <IconFolder size={18} /> },
]

export const SettingsMobileTabs = ({
  activeSection,
  onSectionChange,
  onRefresh,
}: SettingsMobileTabsProps) => {
  const [resetting, setResetting] = useState(false)

  const handleResetStorage = () => {
    modals.openConfirmModal({
      title: (
        <Box style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ThemeIcon color="red" variant="light" size="md">
            <IconAlertTriangle size={16} />
          </ThemeIcon>
          <Text fw={600}>Reset Storage</Text>
        </Box>
      ),
      children: (
        <Stack gap="sm">
          <Text size="sm">
            This will permanently delete all data:
          </Text>
          <Text size="sm" c="dimmed">
            • All providers and tokens
          </Text>
          <Text size="sm" c="dimmed">
            • All cached data and metrics
          </Text>
          <Text size="sm" c="red" fw={500} mt="xs">
            This action cannot be undone.
          </Text>
        </Stack>
      ),
      labels: { confirm: 'Reset Everything', cancel: 'Cancel' },
      confirmProps: { color: 'red' },
      onConfirm: async () => {
        setResetting(true)
        try {
          const result = await service.factoryReset()

          notifications.show({
            title: 'Reset Complete',
            message: `Removed ${result.providers_removed} providers.`,
            color: 'green',
          })

          if (onRefresh) {
            await onRefresh()
          }
        } catch (error: unknown) {
          notifications.show({
            title: 'Reset Failed',
            message: error instanceof Error ? error.message : 'Failed to reset',
            color: 'red',
          })
        } finally {
          setResetting(false)
        }
      },
    })
  }

  return (
    <Box
      style={{
        borderBottom: '1px solid var(--mantine-color-default-border)',
        flexShrink: 0,
      }}
    >
      <ScrollArea
        type="never"
        offsetScrollbars={false}
        styles={{
          viewport: {
            '& > div': {
              display: 'flex !important',
            },
          },
        }}
      >
        <Box
          style={{
            display: 'flex',
            gap: 4,
            padding: '8px 12px',
            minWidth: 'max-content',
          }}
        >
          {TABS.map((tab) => {
            const isActive = activeSection === tab.id

            return (
              <UnstyledButton
                key={tab.id}
                onClick={() => onSectionChange(tab.id)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  padding: '8px 12px',
                  borderRadius: 6,
                  fontSize: 13,
                  fontWeight: isActive ? 500 : 400,
                  color: isActive
                    ? 'var(--mantine-color-text)'
                    : 'var(--mantine-color-dimmed)',
                  backgroundColor: isActive
                    ? 'var(--mantine-color-default-hover)'
                    : 'transparent',
                  transition: 'all 0.15s ease',
                  whiteSpace: 'nowrap',
                }}
              >
                <Box
                  style={{
                    color: isActive
                      ? 'var(--mantine-color-text)'
                      : 'var(--mantine-color-dimmed)',
                  }}
                >
                  {tab.icon}
                </Box>
                {tab.label}
              </UnstyledButton>
            )
          })}

          <UnstyledButton
            onClick={handleResetStorage}
            disabled={resetting}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              padding: '8px 12px',
              borderRadius: 6,
              fontSize: 13,
              fontWeight: 400,
              color: 'var(--mantine-color-red-6)',
              backgroundColor: 'transparent',
              transition: 'all 0.15s ease',
              whiteSpace: 'nowrap',
              opacity: resetting ? 0.5 : 1,
            }}
          >
            <IconTrash size={18} />
            Reset
          </UnstyledButton>
        </Box>
      </ScrollArea>
    </Box>
  )
}
