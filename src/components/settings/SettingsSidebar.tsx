import { NavLink, ScrollArea, Stack } from '@mantine/core'
import { IconChartBar, IconDatabase, IconFolder, IconPlug, IconSettings } from '@tabler/icons-react'
import { Link } from 'react-router-dom'

export const SETTINGS_SECTIONS = [
  { id: 'general', label: 'General', icon: <IconSettings size={16} /> },
  { id: 'providers', label: 'Providers', icon: <IconPlug size={16} /> },
  { id: 'metrics', label: 'Metrics', icon: <IconChartBar size={16} /> },
  { id: 'cache', label: 'Cache', icon: <IconDatabase size={16} /> },
  { id: 'storage', label: 'Storage', icon: <IconFolder size={16} /> },
] as const

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]['id']

export const isSettingsSection = (value: string | undefined): value is SettingsSection =>
  SETTINGS_SECTIONS.some((section) => section.id === value)

export const SettingsSidebar = ({ activeSection }: { activeSection: SettingsSection }) => (
  <ScrollArea w={280} px="md" py="md" style={{ flexShrink: 0 }}>
    <Stack gap="xs">
      {SETTINGS_SECTIONS.map((section) => (
        <NavLink
          key={section.id}
          component={Link}
          to={`/settings/${section.id}`}
          replace
          active={activeSection === section.id}
          label={section.label}
          leftSection={section.icon}
          variant="light"
        />
      ))}
    </Stack>
  </ScrollArea>
)
