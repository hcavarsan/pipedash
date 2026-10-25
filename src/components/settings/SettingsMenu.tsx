import { Box, Group, Stack, Text, UnstyledButton } from '@mantine/core'
import { IconChevronRight } from '@tabler/icons-react'
import { Link } from 'react-router-dom'

import { SETTINGS_SECTIONS, type SettingsSection } from './SettingsSidebar'

const GROUPS: SettingsSection[][] = [
  ['general', 'notifications', 'providers'],
  ['metrics', 'cache', 'storage'],
  ['about'],
]

export function SettingsMenu() {
  return (
    <Stack gap="lg" pb="md">
      {GROUPS.map((ids) => (
        <Box key={ids.join()} className="ios-group">
          {ids.map((id) => {
            const section = SETTINGS_SECTIONS.find((item) => item.id === id)

            if (!section) {
              return null
            }

            return (
              <UnstyledButton key={id} className="ios-row" component={Link} to={`/settings/${id}`}>
                <Group wrap="nowrap" gap="sm">
                  <Box className="ios-icon-tile" aria-hidden>
                    {section.icon}
                  </Box>
                  <Text size="md" fw={500} flex={1}>
                    {section.label}
                  </Text>
                  <Box c="dimmed" aria-hidden style={{ display: 'flex' }}>
                    <IconChevronRight size={18} />
                  </Box>
                </Group>
              </UnstyledButton>
            )
          })}
        </Box>
      ))}
    </Stack>
  )
}
