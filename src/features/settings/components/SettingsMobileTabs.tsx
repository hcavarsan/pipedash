import { Box, Stack, Text, UnstyledButton } from '@mantine/core'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

import { SETTINGS_SECTIONS, type SettingsSection } from '../../../components/common/SettingsSidebar'

interface SettingsTabLabelProps {
  label: string
  icon: ReactNode
}

const SettingsTabLabel = ({ label, icon }: SettingsTabLabelProps): ReactNode => (
  <Stack gap="4xs" align="center">
    {icon}
    <Text size="xs" fw={500} truncate maw="100%">
      {label}
    </Text>
  </Stack>
)

export const SettingsMobileTabs = ({ activeSection }: { activeSection: SettingsSection }): ReactNode => (
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
            {...(active ? { bg: 'var(--mantine-color-blue-light)' } : {})}
          >
            <SettingsTabLabel label={section.label} icon={section.icon} />
          </UnstyledButton>
        )
      })}
    </Box>
  </Box>
)
