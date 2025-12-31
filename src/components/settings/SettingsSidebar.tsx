import { Box, NavLink, ScrollArea, Stack } from '@mantine/core'
import {
  IconChartBar,
  IconDatabase,
  IconFolder,
  IconPlug,
  IconSettings,
} from '@tabler/icons-react'

export type SettingsSection =
  | 'general'
  | 'providers'
  | 'metrics'
  | 'cache'
  | 'storage'

interface SettingsSidebarProps {
  activeSection: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
}

const SECTIONS: Array<{
  id: SettingsSection;
  label: string;
  icon: React.ReactNode;
}> = [
  { id: 'general', label: 'General', icon: <IconSettings size={16} /> },
  { id: 'providers', label: 'Providers', icon: <IconPlug size={16} /> },
  { id: 'metrics', label: 'Metrics', icon: <IconChartBar size={16} /> },
  { id: 'cache', label: 'Cache', icon: <IconDatabase size={16} /> },
  { id: 'storage', label: 'Storage', icon: <IconFolder size={16} /> },
]

export const SettingsSidebar = ({
  activeSection,
  onSectionChange,
}: SettingsSidebarProps) => {

  return (
    <Stack
      h="100%"
      gap={0}
      style={{
        position: 'relative',
        width: 280,
        borderRight: '1px solid var(--mantine-color-default-border)',
      }}
    >
      <ScrollArea flex={1} px="md" py="md">
        <Stack gap="xs">
          {SECTIONS.map((section) => (
            <NavLink
              key={section.id}
              active={activeSection === section.id}
              label={section.label}
              leftSection={
                <Box style={{ color: 'var(--mantine-color-dimmed)' }}>
                  {section.icon}
                </Box>
              }
              onClick={() => onSectionChange(section.id)}
              color="gray"
              variant="subtle"
              style={{ borderRadius: 6 }}
              styles={{
                root: {
                  padding: '8px 12px',
                },
                label: {
                  fontSize: 13,
                },
              }}
            />
          ))}
        </Stack>
      </ScrollArea>
    </Stack>
  )
}
