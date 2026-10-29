import { IconBell, IconPinned, IconPlug, IconSettings, IconStack2 } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link, useLocation, useNavigate } from 'react-router-dom'

import { useKeyboardOpen } from '../../hooks/useKeyboardInset'
import { resumeRoute, useMobileNavStore } from '../../stores/mobileNavStore'
import { useUnreadNotifications } from '../notifications/NotificationCenter'
import classes from './MobileTabBar.module.css'
import { type MobileTab, mobileChrome } from './mobileChrome'
import { tabAction } from './navigation'
import { scrollActiveToTop } from './scrollToTop'
import { useGoBack } from './useMobileBack'

const TABS: Array<{ id: MobileTab; label: string; ariaLabel: string; to: string; icon: typeof IconBell }> = [
  { id: 'pipelines', label: 'Pipelines', ariaLabel: 'Pipelines', to: '/pipelines', icon: IconStack2 },
  { id: 'pinned', label: 'Pinned', ariaLabel: 'Pinned workflows', to: '/pipelines?view=pinned', icon: IconPinned },
  { id: 'providers', label: 'Providers', ariaLabel: 'Providers', to: '/providers', icon: IconPlug },
  { id: 'notifications', label: 'Alerts', ariaLabel: 'Notifications', to: '/notifications', icon: IconBell },
  { id: 'settings', label: 'Settings', ariaLabel: 'Settings', to: '/settings', icon: IconSettings },
]

export function MobileTabBar(): ReactNode {
  const { pathname, search, state } = useLocation()
  const navigate = useNavigate()
  const goBack = useGoBack()
  const tabs = useMobileNavStore((s) => s.tabs)
  const { tab: activeTab, back } = mobileChrome(pathname, search, state)
  const { unread, dotColor } = useUnreadNotifications()
  const keyboardOpen = useKeyboardOpen()
  return (
    <nav
      className={classes.bar}
      aria-label="Main"
      data-hidden={keyboardOpen || undefined}
      aria-hidden={keyboardOpen || undefined}
      inert={keyboardOpen}
    >
      {TABS.map(({ id, label, ariaLabel, to, icon: Icon }) => {
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
            onClick={(event) => {
              event.preventDefault()

              const action = tabAction(activeTab, id, back !== null)

              if (action === 'switch') {
                const route = resumeRoute(tabs, id, to)

                navigate(route.path, route.from ? { state: { from: route.from } } : undefined)
              } else if (action === 'pop-to-root' && back) {
                goBack(back.to)
              } else {
                scrollActiveToTop()
              }
            }}
          >
            <Icon size={24} stroke={active ? 2 : 1.6} aria-hidden />
            <span className={classes.label}>{label}</span>
            {id === 'notifications' && unread > 0 && dotColor && (
              <span
                className={classes.badge}
                style={{ background: `var(--mantine-color-${dotColor}-filled)` }}
                aria-hidden
              >
                {unread > 99 ? '99+' : unread}
              </span>
            )}
          </Link>
        )
      })}
    </nav>
  )
}
