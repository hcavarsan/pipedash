import { IconBell, IconPinned, IconPlug, IconSettings, IconStack2 } from '@tabler/icons-react'
import { Link, useLocation } from 'react-router-dom'

import { useUnreadNotifications } from '../notifications/NotificationCenter'
import classes from './MobileTabBar.module.css'
import { type MobileTab, mobileChrome } from './mobileChrome'

const TABS: Array<{ id: MobileTab; label: string; ariaLabel: string; to: string; icon: typeof IconBell }> = [
  { id: 'pipelines', label: 'Pipelines', ariaLabel: 'Pipelines', to: '/pipelines', icon: IconStack2 },
  { id: 'pinned', label: 'Pinned', ariaLabel: 'Pinned workflows', to: '/pipelines?view=pinned', icon: IconPinned },
  { id: 'providers', label: 'Providers', ariaLabel: 'Providers', to: '/providers', icon: IconPlug },
  { id: 'notifications', label: 'Alerts', ariaLabel: 'Notifications', to: '/notifications', icon: IconBell },
  { id: 'settings', label: 'Settings', ariaLabel: 'Settings', to: '/settings', icon: IconSettings },
]

export function MobileTabBar() {
  const { pathname, search } = useLocation()
  const { tab: activeTab } = mobileChrome(pathname, search)
  const { unread, dotColor } = useUnreadNotifications()

  return (
    <nav className={classes.bar} aria-label="Main">
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
          >
            <Icon size={22} stroke={active ? 2 : 1.6} aria-hidden />
            <span className={classes.label}>{label}</span>
            {id === 'notifications' && dotColor && (
              <span className={classes.badge} style={{ background: `var(--mantine-color-${dotColor}-filled)` }} />
            )}
          </Link>
        )
      })}
    </nav>
  )
}
