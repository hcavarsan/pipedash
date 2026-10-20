import { Box, SimpleGrid, Stack, Text, UnstyledButton } from '@mantine/core'
import { Link } from 'react-router-dom'

import { SETTINGS_SECTIONS, type SettingsSection } from './SettingsSidebar'

export const SettingsMobileTabs = ({ activeSection }: { activeSection: SettingsSection }) => (
  <Box
    component="nav"
    aria-label="Settings sections"
    p={4}
    style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
  >
    <SimpleGrid cols={SETTINGS_SECTIONS.length} spacing={4}>
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
            py={6}
            miw={0}
            c={active ? 'blue' : 'dimmed'}
            bg={active ? 'var(--mantine-color-blue-light)' : undefined}
          >
            <Stack gap={2} align="center">
              {section.icon}
              <Text size="xs" fw={500} truncate maw="100%">
                {section.label}
              </Text>
            </Stack>
          </UnstyledButton>
        )
      })}
    </SimpleGrid>
  </Box>
)
