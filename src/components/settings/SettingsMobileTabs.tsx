import { Box, Stack, Text, UnstyledButton } from '@mantine/core'
import { Link } from 'react-router-dom'

import { SETTINGS_SECTIONS, type SettingsSection } from './SettingsSidebar'

export const SettingsMobileTabs = ({ activeSection }: { activeSection: SettingsSection }) => (
  <Box
    component="nav"
    aria-label="Settings sections"
    p="3xs"
    style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
  >
    <Box style={{ display: 'flex', gap: 'var(--mantine-spacing-3xs)', overflowX: 'auto' }}>
      {SETTINGS_SECTIONS.map((section) => {
        const active = activeSection === section.id

        return (
          <UnstyledButton
            key={section.id}
            component={Link}
            to={`/settings/${section.id}`}
            replace
            className="interactive-surface"
            aria-current={active ? 'page' : undefined}
            bdrs="sm"
            py="2xs"
            px="sm"
            flex="1 0 auto"
            c={active ? 'blue' : 'dimmed'}
            bg={active ? 'var(--mantine-color-blue-light)' : undefined}
          >
            <Stack gap="4xs" align="center">
              {section.icon}
              <Text size="xs" fw={500} truncate maw="100%">
                {section.label}
              </Text>
            </Stack>
          </UnstyledButton>
        )
      })}
    </Box>
  </Box>
)
