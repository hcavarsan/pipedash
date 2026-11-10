import { IconBell, IconPinned, IconSettings, IconStack2 } from '@tabler/icons-react'
import { type MouseEvent, type ReactNode, useEffect, useState } from 'react'
import { Link, useLocation, useNavigate } from 'react-router-dom'
import { useUnreadNotifications } from '../../features/notifications/components/NotificationCenter'
import { useKeyboardOpen } from '../../hooks/useKeyboardInset'
import { resumeRoute, useMobileNavStore } from '../../stores/mobileNavStore'
import classes from './MobileTabBar.module.css'
import { BROWSE_PATH, type MobileTab, mobileChrome } from './mobileChrome'
import { tabAction, tabBarScroll } from './navigation'
import { scrollActiveToTop } from './scrollToTop'
import { useGoBack } from './useMobileBack'

interface Tab {
  id: MobileTab
  label: string
  ariaLabel: string
  to: string
  icon: typeof IconBell
}

const TABS: Tab[] = [
  { id: 'pipelines', label: 'Browse', ariaLabel: 'Browse', to: BROWSE_PATH, icon: IconStack2 },
  {
    id: 'pinned',
    label: 'Pinned',
    ariaLabel: 'Pinned workflows',
    to: '/pipelines?view=pinned',
    icon: IconPinned,
  },
  {
    id: 'notifications',
    label: 'Alerts',
    ariaLabel: 'Notifications',
    to: '/notifications',
    icon: IconBell,
  },
  { id: 'settings', label: 'Settings', ariaLabel: 'Settings', to: '/settings', icon: IconSettings },
]

function useMinimizedOnScroll(): boolean {
  const [minimized, setMinimized] = useState(false)

  useEffect(() => {
    const main = document.querySelector<HTMLElement>('.mantine-AppShell-main')

    if (!main) {
      return
    }

    let state = { minimized: false, anchor: main.scrollTop }
    const onScroll = () => {
      state = tabBarScroll(state, main.scrollTop)
      setMinimized(state.minimized)
    }

    main.addEventListener('scroll', onScroll, { passive: true })

    return () => main.removeEventListener('scroll', onScroll)
  }, [])

  return minimized
}

interface TabBadgeProps {
  count: number
  color: string
}

function TabBadge({ count, color }: TabBadgeProps): ReactNode {
  return (
    <span className={classes.badge} style={{ background: `var(--mantine-color-${color}-filled)` }} aria-hidden>
      {count > 99 ? '99+' : count}
    </span>
  )
}

export function MobileTabBar(): ReactNode {
  const { pathname, search, state } = useLocation()
  const navigate = useNavigate()
  const goBack = useGoBack()
  const tabs = useMobileNavStore((s) => s.tabs)
  const { tab: activeTab, back } = mobileChrome(pathname, search, state)
  const { unread, dotColor } = useUnreadNotifications()
  const keyboardOpen = useKeyboardOpen()
  const minimized = useMinimizedOnScroll()

  const handleTabClick = (event: MouseEvent<HTMLAnchorElement>, { id, to }: Tab): void => {
    event.preventDefault()

    const action = tabAction(activeTab, id, back !== null)

    if (action === 'pop-to-root') {
      goBack(to)
      return
    }

    if (action === 'scroll-top') {
      scrollActiveToTop()
      return
    }

    const route = resumeRoute(tabs, id, to)

    navigate(route.path, route.from ? { state: { from: route.from, tab: id, depth: route.depth ?? 1 } } : undefined)
  }

  return (
    <nav
      className={classes.bar}
      aria-label="Main"
      data-hidden={keyboardOpen || undefined}
      data-minimized={minimized || undefined}
      aria-hidden={keyboardOpen || undefined}
      inert={keyboardOpen}
    >
      {TABS.map((tab) => {
        const { id, label, ariaLabel, to, icon: Icon } = tab
        const active = id === activeTab
        const unreadLabel = id === 'notifications' && unread > 0 ? `${ariaLabel}, ${unread} unread` : ariaLabel

        return (
          <Link
            key={id}
            to={to}
            className={classes.tab}
            data-active={active || undefined}
            aria-current={active ? 'page' : undefined}
            aria-label={unreadLabel}
            onClick={(event) => handleTabClick(event, tab)}
          >
            <Icon size={24} stroke={active ? 2 : 1.6} aria-hidden />
            <span className={classes.label}>{label}</span>
            {id === 'notifications' && unread > 0 && dotColor && <TabBadge count={unread} color={dotColor} />}
          </Link>
        )
      })}
    </nav>
  )
}
