import { Alert, Button, Group } from '@mantine/core'
import type { ReactNode } from 'react'
import { useNow } from '../../../hooks/useNow'
import { type RunLogTarget, useRetryRunLog } from '../../../queries/useRunLogQueries'
import type { LogDocumentMeta } from '../../../types/logs'
import { formatDuration } from '../../../utils/formatDuration'
import classes from './LogViewer.module.css'

interface LiveMarkerProps {
  meta: Pick<LogDocumentMeta, 'updated_at_ms' | 'refresh_error'>
  target: RunLogTarget
  variant: 'desktop' | 'narrow' | 'phone'
}

export function LiveMarker({ meta, target, variant }: LiveMarkerProps): ReactNode {
  const now = useNow(1000)
  const retry = useRetryRunLog()

  if (meta.refresh_error !== null) {
    return (
      <Alert color="orange" variant="light" mx="sm" mb="xs">
        <Group justify="space-between" wrap="nowrap" gap="sm">
          <span>{retry.error?.message ?? meta.refresh_error}</span>
          <Button size="compact-xs" variant="default" loading={retry.isPending} onClick={() => retry.mutate(target)}>
            Retry
          </Button>
        </Group>
      </Alert>
    )
  }

  return (
    <div className={classes.live} data-variant={variant}>
      <span className={classes.liveDot} aria-hidden />
      <span>Live</span>
      <span aria-hidden>·</span>
      <span>updated {formatDuration(Math.max(0, Math.floor((now - meta.updated_at_ms) / 1000)))} ago</span>
    </div>
  )
}
