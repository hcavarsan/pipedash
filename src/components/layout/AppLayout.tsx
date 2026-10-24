import { AppShell, Box } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { type ReactNode, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { useIsMobile } from '../../hooks/useIsMobile'

import { Header } from './Header'
import { Navbar } from './Navbar'
import { useSidebarResize } from './useSidebarResize'

interface AppLayoutProps {
  children: ReactNode
  onOpenSettings?: () => void
}

export const AppLayout = ({ children, onOpenSettings }: AppLayoutProps) => {
  const location = useLocation()
  const isSettingsPage = location.pathname.startsWith('/settings')
  const { isMobile } = useIsMobile()
  const [mobileOpened, { toggle: toggleMobile, close: closeMobile }] = useDisclosure(false)
  const [locationKey, setLocationKey] = useState(location.key)
  const resizable = !isMobile && !isSettingsPage
  const sidebar = useSidebarResize(resizable)
  const compact = !isMobile && sidebar.compact
  const opened = isMobile ? mobileOpened : !sidebar.compact
  const toggle = isMobile ? toggleMobile : sidebar.toggle

  if (locationKey !== location.key) {
    setLocationKey(location.key)
    closeMobile()
  }

  return (
    <AppShell
      header={{ height: 'calc(70px + var(--safe-top))' }}
      navbar={{
        width: { base: 280, sm: sidebar.navbarWidth },
        breakpoint: 'sm',
        collapsed: { mobile: !mobileOpened, desktop: isSettingsPage },
      }}
      mod={{ 'sidebar-resizing': sidebar.resizing, 'sidebar-instant': sidebar.instant }}
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
          onToggleNavbar={isMobile || !isSettingsPage ? toggle : undefined}
          navbarOpened={opened}
          onOpenSettings={onOpenSettings}
        />
      </AppShell.Header>

      <AppShell.Navbar>
        <Navbar compact={compact} />
        {resizable && <Box className="sidebar-resize-handle" {...sidebar.handleProps} />}
      </AppShell.Navbar>

      <AppShell.Main>{children}</AppShell.Main>
    </AppShell>
  )
}
