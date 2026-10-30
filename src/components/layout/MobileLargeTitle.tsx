import { type ReactNode, useEffect } from 'react'
import { useLocation } from 'react-router-dom'

import { useMobileNavStore } from '../../stores/mobileNavStore'
import classes from './MobileLargeTitle.module.css'
import { mobileChrome } from './mobileChrome'

export function MobileLargeTitle(): ReactNode {
  const { pathname, search, state } = useLocation()
  const pushedTitle = useMobileNavStore((s) => s.title)
  const rememberTitle = useMobileNavStore((s) => s.rememberTitle)
  const title = pushedTitle ?? mobileChrome(pathname, search, state).title

  useEffect(() => {
    rememberTitle(pathname + search, title)
  }, [rememberTitle, pathname, search, title])

  return (
    <h1 className={classes.title} data-page-heading data-large-title tabIndex={-1}>
      {title}
    </h1>
  )
}
