import { Button, Group, ScrollArea } from '@mantine/core'
import { Link } from 'react-router-dom'

import { SETTINGS_SECTIONS, type SettingsSection } from './SettingsSidebar'

export const SettingsMobileTabs = ({ activeSection }: { activeSection: SettingsSection }) => (
  <ScrollArea type="never" flex="none" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
    <Group gap="xs" wrap="nowrap" px="sm" py="xs">
      {SETTINGS_SECTIONS.map((section) => (
        <Button
          key={section.id}
          component={Link}
          to={`/settings/${section.id}`}
          replace
          size="sm"
          variant={activeSection === section.id ? 'light' : 'subtle'}
          color={activeSection === section.id ? 'blue' : 'gray'}
          leftSection={section.icon}
          aria-current={activeSection === section.id ? 'page' : undefined}
        >
          {section.label}
        </Button>
      ))}
    </Group>
  </ScrollArea>
)
