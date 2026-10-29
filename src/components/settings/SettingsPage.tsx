import { Box, Group, ScrollArea, Stack } from '@mantine/core'
import { useState } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { PageHeader } from '../common/PageHeader'
import { SettingsMobileTabs } from './SettingsMobileTabs'
import { SETTINGS_SECTIONS, type SettingsSection, SettingsSidebar } from './SettingsSidebar'
import { AboutSection } from './sections/AboutSection'
import { CacheSection } from './sections/CacheSection'
import { GeneralSection } from './sections/GeneralSection'
import { MetricsSection } from './sections/MetricsSection'
import { NotificationsSection } from './sections/NotificationsSection'
import { ProvidersSection } from './sections/ProvidersSection'
import { StorageSection } from './sections/StorageSection'

interface SettingsPageProps {
  activeSection: SettingsSection
  onRefresh?: () => Promise<void>
}

type SectionContent = (
  onRefresh?: () => Promise<void>,
  onHeaderActionsChange?: (actions: React.ReactNode) => void
) => React.ReactNode

const SECTION_CONTENT: Record<SettingsSection, SectionContent> = {
  general: (onRefresh) => <GeneralSection onRefresh={onRefresh} />,
  notifications: () => <NotificationsSection />,
  providers: (_onRefresh, onHeaderActionsChange) => <ProvidersSection onHeaderActionsChange={onHeaderActionsChange} />,
  metrics: () => <MetricsSection />,
  cache: (onRefresh) => <CacheSection onRefresh={onRefresh} />,
  storage: () => <StorageSection />,
  about: () => <AboutSection />,
}

export const SettingsPage = ({ activeSection, onRefresh }: SettingsPageProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const [headerActions, setHeaderActions] = useState<React.ReactNode>(null)
  const sectionTitle = SETTINGS_SECTIONS.find((section) => section.id === activeSection)?.label

  const content = (
    <ScrollArea flex={1}>
      <Box
        p={mobileShell ? 0 : isMobile ? 'md' : 'xl'}
        {...(mobileShell ? { pt: 'sm', className: 'settings-pushed' } : {})}
        {...(isMobile ? {} : { maw: 720 })}
      >
        {!mobileShell && (
          <Box mb="lg">
            <PageHeader title={sectionTitle} actions={headerActions} />
          </Box>
        )}
        {SECTION_CONTENT[activeSection](onRefresh, setHeaderActions)}
      </Box>
    </ScrollArea>
  )

  if (mobileShell) {
    return content
  }

  if (isMobile) {
    return (
      <Stack gap={0} flex={1} mih={0}>
        <SettingsMobileTabs activeSection={activeSection} />
        {content}
      </Stack>
    )
  }

  return (
    <Group gap={0} align="stretch" wrap="nowrap" flex={1} mih={0} h="100%">
      <SettingsSidebar activeSection={activeSection} />
      {content}
    </Group>
  )
}
