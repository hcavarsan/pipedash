import { Button, Group, Paper, Stack, Text } from '@mantine/core'
import type { UseQueryResult } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { LoadingState } from '../../../components/common/LoadingState'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { StandardModal } from '../../../components/common/StandardModal'
import type { ApplicationCopy, Suggestion } from '../../../types/applications'
import {
  useAcceptSuggestions,
  useApplication,
  useApplicationSuggestions,
  useApplications,
  useEditApplication,
} from '../queries/useApplicationQueries'
import { DEFAULT_APPLICATION_QUERY } from '../utils/defaultApplicationQuery'

interface SuggestionsDrawerProps {
  opened: boolean
  onClose: () => void
  applicationId?: number
}

interface SuggestionsViewProps<T> {
  opened: boolean
  onClose: () => void
  copy: ApplicationCopy | undefined
  query: UseQueryResult<T, Error>
  pick: (data: T) => Suggestion[]
}

function RejectButton({
  applicationId,
  suggestionId,
  copy,
}: {
  applicationId: number
  suggestionId: string
  copy: ApplicationCopy
}): ReactNode {
  const edit = useEditApplication()

  return (
    <Button
      size="xs"
      variant="default"
      loading={edit.isPending}
      onClick={() =>
        edit.mutate({ id: applicationId, edit: { type: 'reject_suggestion', suggestion_id: suggestionId } })
      }
    >
      {copy.reject}
    </Button>
  )
}

function SuggestionList({ visible, copy }: { visible: Suggestion[]; copy: ApplicationCopy }): ReactNode {
  if (visible.length === 0) {
    return (
      <Text size="sm" c="dimmed">
        {copy.suggestions_empty}
      </Text>
    )
  }

  return (
    <Stack gap="sm">
      {visible.map((suggestion) => (
        <SuggestionCard key={suggestion.id} suggestion={suggestion} copy={copy} />
      ))}
    </Stack>
  )
}

function SuggestionCard({ suggestion, copy }: { suggestion: Suggestion; copy: ApplicationCopy }): ReactNode {
  const accept = useAcceptSuggestions()
  const { application_id: applicationId } = suggestion

  return (
    <Paper withBorder radius="sm" p="sm">
      <Stack gap="xs">
        <Text size="sm" fw={600}>
          {suggestion.title}
        </Text>
        <Stack gap="4xs">
          {suggestion.why.map((line) => (
            <Text key={line} size="xs" c="dimmed">
              {line}
            </Text>
          ))}
        </Stack>
        <Group gap="sm">
          <Button size="xs" loading={accept.isPending} onClick={() => accept.mutate([suggestion.id])}>
            {copy.accept}
          </Button>
          {applicationId !== null && (
            <RejectButton applicationId={applicationId} suggestionId={suggestion.id} copy={copy} />
          )}
        </Group>
      </Stack>
    </Paper>
  )
}

function SuggestionsView<T>({ opened, onClose, copy, query, pick }: SuggestionsViewProps<T>): ReactNode {
  const accept = useAcceptSuggestions()
  const visible = query.data === undefined ? [] : pick(query.data)
  const bulk = visible.filter((suggestion) => suggestion.bulk_accept)

  if (!copy) {
    return (
      <StandardModal opened={opened} onClose={onClose} size="md" title="">
        <LoadingState message="Loading suggestions…" variant="compact" />
      </StandardModal>
    )
  }

  const footer = (
    <Group justify="flex-end">
      <Button
        size="sm"
        disabled={bulk.length === 0}
        loading={accept.isPending}
        onClick={() => accept.mutate(bulk.map((suggestion) => suggestion.id))}
      >
        {copy.accept_all}
      </Button>
    </Group>
  )

  return (
    <StandardModal opened={opened} onClose={onClose} size="md" title={copy.suggestions} footer={footer}>
      <QueryBoundary query={query} loading="Loading suggestions…" failure="Couldn't load suggestions">
        {() => <SuggestionList visible={visible} copy={copy} />}
      </QueryBoundary>
    </StandardModal>
  )
}

function ListScopedDrawer({ opened, onClose }: Omit<SuggestionsDrawerProps, 'applicationId'>): ReactNode {
  const list = useApplications(DEFAULT_APPLICATION_QUERY)
  const suggestions = useApplicationSuggestions()

  return (
    <SuggestionsView
      opened={opened}
      onClose={onClose}
      copy={list.data?.copy}
      query={suggestions}
      pick={(data) => data}
    />
  )
}

function DetailScopedDrawer({
  opened,
  onClose,
  applicationId,
}: SuggestionsDrawerProps & { applicationId: number }): ReactNode {
  const detail = useApplication(applicationId)

  return (
    <SuggestionsView
      opened={opened}
      onClose={onClose}
      copy={detail.data?.copy}
      query={detail}
      pick={(data) => data.suggestions}
    />
  )
}

export function SuggestionsDrawer({ opened, onClose, applicationId }: SuggestionsDrawerProps): ReactNode {
  if (applicationId === undefined) {
    return <ListScopedDrawer opened={opened} onClose={onClose} />
  }

  return <DetailScopedDrawer opened={opened} onClose={onClose} applicationId={applicationId} />
}
