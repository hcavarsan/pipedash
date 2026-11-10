import { Button } from '@mantine/core'
import { useReducedMotion } from '@mantine/hooks'
import { IconChevronRight } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../../components/common/EmptyState'
import { PRESENT_EASING } from '../../../../lib/motion'
import { usePlugins } from '../../../../queries/usePluginsQueries'

export const NAME_HINT = 'A friendly name to identify this provider'
export const ADVANCED_LABEL = 'Advanced settings'

export interface AdvancedSection {
  available: boolean
  shown: boolean
  onToggle: () => void
  content: ReactNode
}

export function AdvancedChevron({ shown }: { shown: boolean }): ReactNode {
  const reduceMotion = useReducedMotion()

  return (
    <IconChevronRight
      size={16}
      aria-hidden
      style={
        reduceMotion
          ? undefined
          : { transform: shown ? 'rotate(90deg)' : undefined, transition: `transform 120ms ${PRESENT_EASING}` }
      }
    />
  )
}

export function NoProviderTypes(): ReactNode {
  const { refetch, isFetching } = usePlugins()

  return (
    <EmptyState
      title="No provider types available"
      description="Pipedash has no providers it can connect to."
      action={
        <Button variant="default" loading={isFetching} onClick={() => void refetch()}>
          Retry
        </Button>
      }
    />
  )
}
