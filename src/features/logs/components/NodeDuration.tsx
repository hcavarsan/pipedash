import type { ReactNode } from 'react'
import { useNow } from '../../../hooks/useNow'
import type { RunLogNode } from '../../../types/logs'
import { formatDuration } from '../../../utils/formatDuration'

function Elapsed({ startedAt }: { startedAt: string }): ReactNode {
  const now = useNow(1000)

  return formatDuration(Math.max(0, (now - Date.parse(startedAt)) / 1000))
}

export function NodeDuration({ node }: { node: RunLogNode }): ReactNode {
  return node.status === 'running' && node.started_at ? (
    <Elapsed startedAt={node.started_at} />
  ) : (
    formatDuration(node.duration_seconds)
  )
}
