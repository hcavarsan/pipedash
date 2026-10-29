import { Box, Group, ScrollArea, Stack } from '@mantine/core'
import type { ReactNode } from 'react'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { PageHeader } from '../common/PageHeader'
import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import { SettingsMobileTabs } from './SettingsMobileTabs'
import { SETTINGS_SECTIONS, type SettingsSection, SettingsSidebar } from './SettingsSidebar'
import { AboutSection } from './sections/AboutSection'
import { CacheSection } from './sections/CacheSection'
import { GeneralSection } from './sections/GeneralSection'
import { MetricsSection } from './sections/MetricsSection'
import { NotificationsSection } from './sections/NotificationsSection'
import { ProvidersSection } from './sections/ProvidersSection'
import { ProviderTransferActions } from './sections/ProviderTransferActions'
import { StorageSection } from './sections/StorageSection'

interface SettingsPageProps {
  activeSection: SettingsSection
  onRefresh?: () => Promise<void>
}

type SectionContent = (onRefresh?: () => Promise<void>) => ReactNode

const SECTION_CONTENT: Record<SettingsSection, SectionContent> = {
  general: (onRefresh) => <GeneralSection onRefresh={onRefresh} />,
  notifications: () => <NotificationsSection />,
  providers: () => <ProvidersSection />,
  metrics: () => <MetricsSection />,
  cache: (onRefresh) => <CacheSection onRefresh={onRefresh} />,
  storage: () => <StorageSection />,
  about: () => <AboutSection />,
}

export const SettingsPage = ({ activeSection, onRefresh }: SettingsPageProps): ReactNode => {
  const { isMobile } = useIsMobile()
  const mobileShell = useMobileShell()
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
            <PageHeader
              title={sectionTitle}
              actions={activeSection === 'providers' ? <ProviderTransferActions /> : null}
            />
          </Box>
        )}
        <ErrorBoundary key={activeSection} scope="feature" name={sectionTitle ?? 'Settings'}>
          {SECTION_CONTENT[activeSection](onRefresh)}
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
