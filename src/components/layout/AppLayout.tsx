import { ReactNode, useEffect, useState } from 'react'

import { AppShell, Box, Tooltip } from '@mantine/core'
import { IconChevronsRight } from '@tabler/icons-react'

import { useIsMobile } from '../../contexts/MediaQueryContext'
import type { ProviderConfig, ProviderSummary } from '../../types'
import { AddProviderModal } from '../provider/AddProviderModal'
import { SettingsModal } from '../provider/SettingsModal'

import { Header } from './Header'
import { MobileFooter } from './MobileFooter'
import { Navbar } from './Navbar'

interface AppLayoutProps {
  children: ReactNode;
  selectedProviderId?: number;
  onProviderSelect?: (id: number | undefined) => void;
  providers: ProviderSummary[];
  providersLoading?: boolean;
  providersError?: string | null;
  onAddProvider: (config: ProviderConfig) => Promise<void>;
  onUpdateProvider: (id: number, config: ProviderConfig) => Promise<void>;
  onRemoveProvider: (id: number, name: string) => Promise<void>;
  onRefreshAll?: () => void;
  onRefreshProviders?: () => Promise<void>;
  refreshing?: boolean;
}

export const AppLayout = ({
  children,
  selectedProviderId,
  onProviderSelect,
  providers,
  providersLoading = false,
  providersError = null,
  onAddProvider,
  onUpdateProvider,
  onRemoveProvider,
  onRefreshAll,
  onRefreshProviders,
  refreshing = false,
}: AppLayoutProps) => {
  const isMobile = useIsMobile()
  const [opened, setOpened] = useState(false)
  const [settingsOpened, setSettingsOpened] = useState(false)
  const [addProviderModalOpened, setAddProviderModalOpened] = useState(false)

  useEffect(() => {
    if (!isMobile) {
      setOpened(true)
    }
  }, [isMobile])

  const toggle = () => setOpened(prev => !prev)

  return (
    <AppShell
      header={{ height: isMobile ? 'calc(70px + env(safe-area-inset-top))' : 70 }}
      navbar={{
        width: 280,
        breakpoint: 'sm',
        collapsed: { mobile: !opened, desktop: !opened },
      }}
      padding={{ base: 'xs', sm: 'md' }}
    >
      <AppShell.Header>
        <Header
          onRefreshAll={onRefreshAll}
          onToggleNavbar={toggle}
          navbarOpened={opened}
          onOpenSettings={() => setSettingsOpened(true)}
          refreshing={refreshing}
        />
      </AppShell.Header>

      <AppShell.Navbar>
        <Navbar
          selectedProviderId={selectedProviderId}
          onProviderSelect={onProviderSelect}
          providers={providers}
          loading={providersLoading}
          error={providersError}
          onAddProvider={onAddProvider}
          onUpdateProvider={onUpdateProvider}
          onRemoveProvider={onRemoveProvider}
          onToggleSidebar={toggle}
          sidebarOpened={opened}
        />
      </AppShell.Navbar>

      <AppShell.Main>
        {/* Expand tab - stuck to left edge at top (only when sidebar is collapsed) - Hidden on mobile */}
        {!opened && !isMobile && (
          <Tooltip label="Show sidebar" position="right" withArrow>
            <Box
              onClick={toggle}
              style={{
                position: 'fixed',
                top: 74,
                left: 0,
                zIndex: 100,
                width: 28,
                height: 28,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: 'var(--mantine-color-body)',
                border: '1px solid var(--mantine-color-default-border)',
                borderRadius: 6,
                cursor: 'pointer',
                transition: 'all 0.15s ease',
                boxShadow: '0 2px 4px rgba(0, 0, 0, 0.08)',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = 'var(--mantine-color-gray-0)'
                e.currentTarget.style.transform = 'scale(1.05)'
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = 'var(--mantine-color-body)'
                e.currentTarget.style.transform = 'scale(1)'
              }}
            >
              <IconChevronsRight size={14} style={{ color: 'var(--mantine-color-dimmed)' }} />
            </Box>
          </Tooltip>
        )}
        {children}
      </AppShell.Main>

      <SettingsModal
        opened={settingsOpened}
        onClose={() => setSettingsOpened(false)}
        providers={providers}
        loading={providersLoading}
        error={providersError}
        onRemoveProvider={onRemoveProvider}
        onUpdateProvider={onUpdateProvider}
        onRefresh={onRefreshProviders}
      />

      <AddProviderModal
        opened={addProviderModalOpened}
        onClose={() => setAddProviderModalOpened(false)}
        onAdd={onAddProvider}
      />

      <MobileFooter onAddProvider={() => setAddProviderModalOpened(true)} />
    </AppShell>
  )
}
