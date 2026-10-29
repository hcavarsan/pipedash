import { AppShell, Box } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { useIsMobile, useMobileShell } from '../../hooks/useIsMobile'
import { useKeyboardOpen } from '../../hooks/useKeyboardInset'

import { ErrorBoundary } from '../ErrorBoundary/ErrorBoundary'
import { NotificationHost } from '../notifications/NotificationCenter'
import { ProviderModals } from '../provider/ProviderModals'
import { PipelineViewModals } from '../table/PipelineViewModals'
import classes from './AppLayout.module.css'
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

export const AppLayout = ({ children, onOpenSettings }: AppLayoutProps): ReactNode => {
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
  const mainRef = useRef<HTMLElement>(null)
  const headerRef = useRef<HTMLElement>(null)
  const listPositions = useRef(new Map<string, { main: number; viewports: number[] }>())
  const lastPage = useRef<string | null>(null)

  useEffect(() => {
    if (mobileShell) {
      return
    }
    const main = mainRef.current
    if (!main) {
      return
    }
    const route = `${location.pathname}${location.search}`
    const page = `${location.pathname}?view=${new URLSearchParams(location.search).get('view') ?? ''}`
    const samePage = lastPage.current === page
    const saved = location.pathname === '/pipelines' ? listPositions.current.get(route) : undefined
    const position = {
      main: samePage ? main.scrollTop : (saved?.main ?? 0),
      viewports: [...(saved?.viewports ?? [])],
    }
    const restored = new WeakSet<HTMLElement>()
    let focused = false
    lastPage.current = page
    const updateEdge = (event?: Event) => {
      if (!event || event.target === main) {
        position.main = main.scrollTop
        headerRef.current?.toggleAttribute('data-scrolled', position.main > 0)
      } else {
        main.querySelectorAll<HTMLElement>('[data-scrollarea-viewport]').forEach((viewport, index) => {
          if (event.target === viewport) {
            position.viewports[index] = viewport.scrollTop
          }
        })
      }
    }
    const arrive = () => {
      main.querySelectorAll<HTMLElement>('[data-scrollarea-viewport]').forEach((viewport, index) => {
        if (!restored.has(viewport) && viewport.scrollHeight > viewport.clientHeight) {
          viewport.scrollTop = saved?.viewports[index] ?? 0
          restored.add(viewport)
        }
      })
      if (focused) {
        return
      }
      const heading = main.querySelector<HTMLElement>('[data-page-heading]')
      if (heading) {
        heading.focus({ preventScroll: true })
        focused = true
      }
    }
    const frame = requestAnimationFrame(() => {
      if (!samePage) {
        main.scrollTop = position.main
        arrive()
      }
      updateEdge()
    })
    const observer = new MutationObserver(arrive)
    if (!samePage) {
      observer.observe(main, { childList: true, subtree: true })
    }
    main.addEventListener('scroll', updateEdge, { passive: true, capture: true })

    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
      main.removeEventListener('scroll', updateEdge, true)
      if (location.pathname === '/pipelines') {
        listPositions.current.set(route, position)
      }
    }
  }, [mobileShell, location.pathname, location.search])

  if (locationKey !== location.key) {
    setLocationKey(location.key)
    closeMobile()
  }

  const tabBarReserve = keyboardOpen ? '0px' : 'calc(82px + var(--safe-bottom))'

  return (
    <>
      {!mobileShell && (
        <button
          type="button"
          className={classes.skipLink}
          onClick={() => mainRef.current?.focus({ preventScroll: true })}
        >
          Skip to content
        </button>
      )}
      <AppShell
        header={{ height: mobileShell ? 'calc(44px + var(--safe-top))' : 'calc(70px + var(--safe-top))' }}
        {...(mobileShell
          ? {}
          : {
              navbar: {
                width: { base: 280, sm: sidebar.navbarWidth },
                breakpoint: 'sm',
                collapsed: { mobile: !mobileOpened, desktop: isSettingsPage },
              },
            })}
        mod={{
          'sidebar-resizing': sidebar.resizing,
          'sidebar-instant': sidebar.instant,
          'mobile-shell': mobileShell,
        }}
        padding={{ base: 'xs', sm: 'md' }}
        {...(mobileShell ? { style: { '--tab-bar-reserve': tabBarReserve } } : {})}
        styles={{
          header: mobileShell
            ? {
                background: 'transparent',
                paddingInline: 'max(var(--safe-left), var(--safe-right))',
                borderBottom: 0,
                backdropFilter: 'none',
                WebkitBackdropFilter: 'none',
                transition: 'none',
              }
            : {},
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
        <AppShell.Header ref={headerRef} className={mobileShell ? undefined : classes.header}>
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
            <ErrorBoundary scope="feature" name="Sidebar">
              <Navbar compact={compact} />
            </ErrorBoundary>
            {resizable && <Box className="sidebar-resize-handle" {...sidebar.handleProps} />}
          </AppShell.Navbar>
        )}

        <AppShell.Main
          ref={mainRef}
          id={mobileShell ? undefined : 'main-content'}
          tabIndex={mobileShell ? undefined : -1}
        >
          {mobileShell ? <MobilePage>{children}</MobilePage> : children}
        </AppShell.Main>

        {mobileShell && <MobileTabBar />}
        <ErrorBoundary scope="feature" name="Provider dialogs">
          <ProviderModals />
        </ErrorBoundary>
        <ErrorBoundary scope="feature" name="Pipeline view dialogs">
          <PipelineViewModals />
        </ErrorBoundary>
        <ErrorBoundary scope="feature" name="Notifications">
          <NotificationHost />
        </ErrorBoundary>
      </AppShell>
    </>
  )
}
