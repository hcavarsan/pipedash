import { ActionIcon, Tooltip } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'

export interface ReadFailure {
  key: string
  title: string
  error: unknown
  retrying: boolean
  onRetry: () => void
}

export const readFailureOf = (key: string, title: string, query: UseQueryResult<unknown, Error>): ReadFailure[] =>
  query.isError
    ? [{ key, title, error: query.error, retrying: query.isFetching, onRetry: () => void query.refetch() }]
    : []

export const NavbarReadFailures = ({ failures }: { failures: ReadFailure[] }): ReactNode =>
  failures.map(({ key, title, error, retrying, onRetry }) => (
    <ErrorState key={key} title={title} error={error} onRetry={onRetry} retrying={retrying} />
  ))

export const RailReadFailures = ({ failures }: { failures: ReadFailure[] }): ReactNode =>
  failures.map(({ key, title, retrying, onRetry }) => (
    <Tooltip key={key} label={`${title}. Retry`} position="right">
      <ActionIcon size={44} radius="sm" color="red" aria-label={`${title}. Retry`} loading={retrying} onClick={onRetry}>
        <IconAlertTriangle size={20} aria-hidden />
      </ActionIcon>
    </Tooltip>
  ))
