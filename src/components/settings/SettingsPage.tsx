import { Box, Group, ScrollArea, Stack } from '@mantine/core'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { SettingsMobileTabs } from './SettingsMobileTabs'
import { type SettingsSection, SettingsSidebar } from './SettingsSidebar'
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

const SECTION_CONTENT: Record<SettingsSection, (onRefresh?: () => Promise<void>) => React.ReactNode> = {
  general: (onRefresh) => <GeneralSection onRefresh={onRefresh} />,
  notifications: () => <NotificationsSection />,
  providers: () => <ProvidersSection />,
  metrics: () => <MetricsSection />,
  cache: (onRefresh) => <CacheSection onRefresh={onRefresh} />,
  storage: () => <StorageSection />,
  about: () => <AboutSection />,
}

export const SettingsPage = ({ activeSection, onRefresh }: SettingsPageProps) => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()

  const content = (
    <ScrollArea flex={1}>
      <Box
        p={isMobile ? 'md' : 'xl'}
        maw={isMobile ? undefined : 720}
        className={mobileShell ? 'settings-pushed' : undefined}
      >
        {SECTION_CONTENT[activeSection](onRefresh)}
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
