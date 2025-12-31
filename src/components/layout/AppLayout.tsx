import { ReactNode, useEffect, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { AppShell } from '@mantine/core'

import { useIsMobile } from '../../hooks/useIsMobile'
import { useProviderStore } from '../../stores/providerStore'

import { Header } from './Header'
import { MobileFooter } from './MobileFooter'
import { Navbar } from './Navbar'

interface AppLayoutProps {
  children: ReactNode
  onRefreshAll?: () => void
  refreshing?: boolean
  onOpenSettings?: () => void
}

export const AppLayout = ({
  children,
  onRefreshAll,
  refreshing = false,
  onOpenSettings,
}: AppLayoutProps) => {
  const location = useLocation()
  const isSettingsPage = location.pathname.startsWith('/settings')
  const { isMobile } = useIsMobile()
  const [opened, setOpened] = useState(() => !isMobile)

  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)

  useEffect(() => {
    if (!isMobile) {
      setOpened(true)
    }
  }, [isMobile])

  const toggle = () => setOpened((prev) => !prev)

  return (
    <>
      <AppShell
        header={{ height: isMobile ? 'calc(70px + env(safe-area-inset-top))' : 70 }}
        navbar={{
          width: 280,
          breakpoint: 'sm',
          collapsed: { mobile: !opened, desktop: !opened || isSettingsPage },
        }}
        padding={{ base: 'xs', sm: 'md' }}
        styles={{
          main: {
            minHeight: 'unset',
            background: 'var(--mantine-color-body)',
          },
        }}
      >
        <AppShell.Header>
          <Header
            onRefreshAll={onRefreshAll}
            onToggleNavbar={toggle}
            navbarOpened={opened}
            onOpenSettings={onOpenSettings}
            refreshing={refreshing}
          />
        </AppShell.Header>

        <AppShell.Navbar>
          <Navbar onToggleSidebar={toggle} sidebarOpened={opened} />
        </AppShell.Navbar>

        <AppShell.Main>
          {children}
        </AppShell.Main>
      </AppShell>

      <MobileFooter onAddProvider={openAddProviderModal} />
    </>
  )
}
