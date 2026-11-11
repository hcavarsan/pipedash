import type { ReactNode } from 'react'
import { useLocation } from 'react-router-dom'

import { useMobileNavStore } from '../../stores/mobileNavStore'
import classes from './MobileLargeTitle.module.css'
import { mobileChrome, routeKey } from './mobileChrome'

export function MobileLargeTitle(): ReactNode {
  const { pathname, search, state } = useLocation()
  const key = routeKey(pathname, search, state)
  const title = useMobileNavStore((s) => s.titles[key]) ?? mobileChrome(pathname, search, state).title

  return (
    <h1 className={classes.title} data-page-heading data-large-title tabIndex={-1}>
      {title}
    </h1>
  )
}
