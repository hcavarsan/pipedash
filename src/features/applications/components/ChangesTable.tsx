import { Button, Code, Group, ScrollArea, Stack, Table, Text } from '@mantine/core'
import type { ReactNode } from 'react'
import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import type { ApplicationCopy, ChangeColumn, ChangeRow } from '../../../types/applications'
import { useApplicationChanges } from '../queries/useApplicationQueries'
import { ChangeCellButton } from './ChangeCellButton'
import { TimeSegments } from './TimeSegments'

interface ChangesTableProps {
  applicationId: number
  copy: ApplicationCopy
}

function CommitCell({ row }: { row: ChangeRow }): ReactNode {
  return (
    <Stack gap="3xs" miw={200}>
      <Group gap="xs" wrap="nowrap">
        <Code>{row.commit_short}</Code>
        {row.message !== null && (
          <Text size="sm" truncate maw={360}>
            {row.message}
          </Text>
        )}
      </Group>
      <TimeSegments segments={row.segments} />
    </Stack>
  )
}

function ChangeTableRow({ row, columns }: { row: ChangeRow; columns: ChangeColumn[] }): ReactNode {
  return (
    <Table.Tr>
      <Table.Td>
        <CommitCell row={row} />
      </Table.Td>
      {columns.map((column) => {
        const cell = row.cells.find((candidate) => candidate.column === column.key)

        return <Table.Td key={column.key}>{cell && <ChangeCellButton cell={cell} />}</Table.Td>
      })}
    </Table.Tr>
  )
}

function ChangeTableHead({ copy, columns }: { copy: ApplicationCopy; columns: ChangeColumn[] }): ReactNode {
  return (
    <Table.Tr>
      <Table.Th>{copy.commit_column}</Table.Th>
      {columns.map((column) => (
        <Table.Th key={column.key}>{column.label}</Table.Th>
      ))}
    </Table.Tr>
  )
}

export function ChangesTable({ applicationId, copy }: ChangesTableProps): ReactNode {
  const query = useApplicationChanges(applicationId)

  if (query.isError && !query.data) {
    return <ErrorState title="Couldn't load changes" error={query.error} onRetry={() => void query.refetch()} />
  }

  if (!query.data) {
    return <LoadingState message="Loading changes…" />
  }

  const columns = query.data.pages[0]?.columns ?? []
  const rows = query.data.pages.flatMap((page) => page.rows)

  if (rows.length === 0) {
    return (
      <Text c="dimmed" ta="center" py="xl" role="status">
        {copy.changes_empty}
      </Text>
    )
  }

  return (
    <Stack gap="sm">
      <ScrollArea>
        <Table highlightOnHover>
          <Table.Thead>
            <ChangeTableHead copy={copy} columns={columns} />
          </Table.Thead>
          <Table.Tbody>
            {rows.map((row) => (
              <ChangeTableRow key={row.commit_sha} row={row} columns={columns} />
            ))}
          </Table.Tbody>
        </Table>
      </ScrollArea>
      {query.hasNextPage && (
        <Group justify="center">
          <Button variant="default" loading={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
            {copy.load_more}
          </Button>
        </Group>
      )}
    </Stack>
  )
}
