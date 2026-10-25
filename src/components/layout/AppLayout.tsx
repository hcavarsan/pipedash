import { AppShell, Box } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { type CSSProperties, type ReactNode, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useKeyboardOpen } from '../../hooks/useKeyboardInset'

import { NotificationHost } from '../notifications/NotificationCenter'
import { ProviderModals } from '../provider/ProviderModals'

import { Header } from './Header'
import { MobileHeader } from './MobileHeader'
import { MobilePage } from './MobilePage'
import { MobileTabBar } from './MobileTabBar'
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
  const mobileShell = useMobileShell()
  const keyboardOpen = useKeyboardOpen()
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

  const tabBarReserve = keyboardOpen ? '0px' : 'calc(82px + var(--safe-bottom))'

  return (
    <AppShell
      header={{ height: mobileShell ? 'calc(44px + var(--safe-top))' : 'calc(70px + var(--safe-top))' }}
      navbar={
        mobileShell
          ? undefined
          : {
              width: { base: 280, sm: sidebar.navbarWidth },
              breakpoint: 'sm',
              collapsed: { mobile: !mobileOpened, desktop: isSettingsPage },
            }
      }
      mod={{
        'sidebar-resizing': sidebar.resizing,
        'sidebar-instant': sidebar.instant,
        'mobile-shell': mobileShell,
      }}
      padding={{ base: 'xs', sm: 'md' }}
      style={mobileShell ? ({ '--tab-bar-reserve': tabBarReserve } as CSSProperties) : undefined}
      styles={{
        header: mobileShell
          ? {
              background: 'transparent',
              borderBottom: 0,
              backdropFilter: 'none',
              WebkitBackdropFilter: 'none',
              transition: 'none',
            }
          : undefined,
        main: {
          minHeight: 'unset',
          background: 'var(--mantine-color-body)',
          ...(mobileShell
            ? {
                paddingBottom: 'calc(var(--app-shell-padding) + var(--tab-bar-reserve) + var(--keyboard-inset, 0px))',
              }
            : undefined),
        },
      }}
    >
      <AppShell.Header>
        {mobileShell ? (
          <MobileHeader />
        ) : (
          <Header
            onToggleNavbar={isMobile || !isSettingsPage ? toggle : undefined}
            navbarOpened={opened}
            onOpenSettings={onOpenSettings}
          />
        )}
      </AppShell.Header>

      {!mobileShell && (
        <AppShell.Navbar>
          <Navbar compact={compact} />
          {resizable && <Box className="sidebar-resize-handle" {...sidebar.handleProps} />}
        </AppShell.Navbar>
      )}

      <AppShell.Main>{mobileShell ? <MobilePage>{children}</MobilePage> : children}</AppShell.Main>

      {mobileShell && <MobileTabBar />}
      <ProviderModals />
      <NotificationHost />
    </AppShell>
  )
}
