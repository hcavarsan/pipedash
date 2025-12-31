import { Box, ScrollArea } from '@mantine/core'

import { useIsMobile } from '../../hooks/useIsMobile'

import { CacheSection } from './sections/CacheSection'
import { GeneralSection } from './sections/GeneralSection'
import { MetricsSection } from './sections/MetricsSection'
import { ProvidersSection } from './sections/ProvidersSection'
import { StorageSection } from './sections/StorageSection'
import { SettingsMobileTabs } from './SettingsMobileTabs'
import { type SettingsSection, SettingsSidebar } from './SettingsSidebar'

interface SettingsPageProps {
  activeSection: SettingsSection
  onSectionChange: (section: SettingsSection) => void
  onRefresh?: () => Promise<void>
}

export const SettingsPage = ({
  activeSection,
  onSectionChange,
  onRefresh,
}: SettingsPageProps) => {
  const { isMobile } = useIsMobile()

  const renderSection = () => {
    switch (activeSection) {
      case 'general':
        return <GeneralSection onRefresh={onRefresh} />
      case 'providers':
        return <ProvidersSection />
      case 'metrics':
        return <MetricsSection />
      case 'cache':
        return <CacheSection onRefresh={onRefresh} />
      case 'storage':
        return <StorageSection />
      default:
        return null
    }
  }

  return (
    <>
      {isMobile ? (
        <Box style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
          <SettingsMobileTabs
            activeSection={activeSection}
            onSectionChange={onSectionChange}
            onRefresh={onRefresh}
          />
          <ScrollArea
            style={{ flex: 1 }}
            styles={{
              viewport: {
                '& > div': {
                  minHeight: '100%',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                },
              },
            }}
          >
            <Box p="md" pb="xl">
              {renderSection()}
            </Box>
          </ScrollArea>
        </Box>
      ) : (
        <Box
          style={{
            flex: 1,
            display: 'flex',
            alignItems: 'stretch',
            minHeight: 0,
            height: '100%',
          }}
        >
          <SettingsSidebar activeSection={activeSection} onSectionChange={onSectionChange} />

          <Box
            style={{
              flex: 1,
              alignSelf: 'stretch',
              display: 'flex',
              flexDirection: 'column',
              minWidth: 0,
              minHeight: 0,
            }}
          >
            <ScrollArea
              style={{ flex: 1 }}
              styles={{
                viewport: {
                  '& > div': {
                    minHeight: '100%',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  },
                },
              }}
            >
              <Box
                p="xl"
                style={{
                  maxWidth: 720,
                  width: '100%',
                }}
              >
                {renderSection()}
              </Box>
            </ScrollArea>
          </Box>
        </Box>
      )}
    </>
  )
}
