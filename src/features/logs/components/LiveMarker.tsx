import type { ReactNode } from 'react'
import { useNow } from '../../../hooks/useNow'
import { formatDuration } from '../../../utils/formatDuration'
import classes from './LogViewer.module.css'

interface LiveMarkerProps {
  updatedAtMs: number
  variant: 'desktop' | 'narrow' | 'phone'
}

export function LiveMarker({ updatedAtMs, variant }: LiveMarkerProps): ReactNode {
  const now = useNow(1000)

  return (
    <div className={classes.live} data-variant={variant}>
      <span className={classes.liveDot} aria-hidden />
      <span>Live</span>
      <span aria-hidden>·</span>
      <span>updated {formatDuration(Math.max(0, Math.floor((now - updatedAtMs) / 1000)))} ago</span>
    </div>
  )
}
