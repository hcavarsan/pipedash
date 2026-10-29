import type { ReactNode } from 'react'
import { useLocation } from 'react-router-dom'

import classes from './MobileLargeTitle.module.css'
import { mobileChrome } from './mobileChrome'

export function MobileLargeTitle(): ReactNode {
  const { pathname, search, state } = useLocation()
  const { title, back } = mobileChrome(pathname, search, state)

  if (back) {
    return null
  }

  return (
    <h1 className={classes.title} data-page-heading data-large-title tabIndex={-1}>
      {title}
    </h1>
  )
}
