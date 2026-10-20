import { AppShell } from '@mantine/core'
import { useDisclosure, useLocalStorage } from '@mantine/hooks'
import type { ReactNode } from 'react'
import { useLocation } from 'react-router-dom'

import { useIsMobile } from '../../hooks/useIsMobile'

import { Header } from './Header'
import { Navbar } from './Navbar'

interface AppLayoutProps {
  children: ReactNode
  onRefreshAll?: () => void
  refreshing?: boolean
  onOpenSettings?: () => void
}

export const AppLayout = ({ children, onRefreshAll, refreshing = false, onOpenSettings }: AppLayoutProps) => {
  const location = useLocation()
  const isSettingsPage = location.pathname.startsWith('/settings')
  const { isMobile } = useIsMobile()
  const [mobileOpened, { toggle: toggleMobile }] = useDisclosure(false)
  const [desktopCompact, setDesktopCompact] = useLocalStorage({ key: 'pipedash-sidebar-compact', defaultValue: false })
  const compact = !isMobile && desktopCompact
  const opened = isMobile ? mobileOpened : !desktopCompact
  const toggle = isMobile ? toggleMobile : () => setDesktopCompact((value) => !value)

  return (
    <AppShell
      header={{ height: isMobile ? 'calc(70px + env(safe-area-inset-top))' : 70 }}
      navbar={{
        width: { base: 280, sm: compact ? 68 : 248 },
        breakpoint: 'sm',
        collapsed: { mobile: !mobileOpened, desktop: isSettingsPage },
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
          onToggleNavbar={isMobile || !isSettingsPage ? toggle : undefined}
          navbarOpened={opened}
          onOpenSettings={onOpenSettings}
          refreshing={refreshing}
        />
      </AppShell.Header>

      <AppShell.Navbar>
        <Navbar compact={compact} onToggleSidebar={toggle} sidebarOpened={opened} />
      </AppShell.Navbar>

      <AppShell.Main>{children}</AppShell.Main>
    </AppShell>
  )
}
