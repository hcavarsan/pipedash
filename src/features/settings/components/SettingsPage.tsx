import { Box, Group, ScrollArea, Stack } from '@mantine/core'
import type { ReactNode } from 'react'
import { PageHeader } from '../../../components/common/PageHeader'
import { SETTINGS_SECTIONS, type SettingsSection, SettingsSidebar } from '../../../components/common/SettingsSidebar'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { useIsMobile, useMobileShell } from '../../../hooks/useIsMobile'
import { MetricsSection } from '../../metrics/components/MetricsSection'
import { NotificationsSection } from '../../notifications/components/NotificationsSection'
import { ProvidersSection } from '../../providers/components/ProvidersSection'
import { ProviderTransferActions } from '../../providers/components/ProviderTransferActions'
import { AboutSection } from './AboutSection'
import { CacheSection } from './CacheSection'
import { DevicesSection } from './DevicesSection'
import { GeneralSection } from './GeneralSection'
import { SettingsMobileTabs } from './SettingsMobileTabs'
import { StorageSection } from './StorageSection'

interface SettingsPageProps {
  activeSection: SettingsSection
  onRefresh?: () => Promise<void>
}

function sectionContent(section: SettingsSection, onRefresh?: () => Promise<void>): ReactNode {
  switch (section) {
    case 'general':
      return <GeneralSection onRefresh={onRefresh} />
    case 'notifications':
      return <NotificationsSection />
    case 'providers':
      return <ProvidersSection />
    case 'devices':
      return <DevicesSection />
    case 'metrics':
      return <MetricsSection />
    case 'cache':
      return <CacheSection onRefresh={onRefresh} />
    case 'storage':
      return <StorageSection />
    case 'about':
      return <AboutSection />
  }
}

export const SettingsPage = ({ activeSection, onRefresh }: SettingsPageProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
  const sectionTitle = SETTINGS_SECTIONS.find((section) => section.id === activeSection)?.label

  let padding: string | number = 'xl'
  if (mobileShell) {
    padding = 0
  } else if (isMobile) {
    padding = 'md'
  }
  const content = (
    <ScrollArea flex={1}>
      <Box
        p={padding}
        {...(mobileShell ? { pt: 'sm', className: 'settings-pushed' } : {})}
        {...(isMobile ? {} : { maw: 880 })}
      >
        {!mobileShell && (
          <Box mb="lg">
            <PageHeader
              title={sectionTitle}
              actions={activeSection === 'providers' ? <ProviderTransferActions /> : null}
            />
          </Box>
        )}
        <ErrorBoundary key={activeSection} scope="feature" name={sectionTitle ?? 'Settings'}>
          {sectionContent(activeSection, onRefresh)}
        </ErrorBoundary>
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
