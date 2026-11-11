import { Alert, Badge, Button, Group, Stack, Text } from '@mantine/core'
import { useDisclosure } from '@mantine/hooks'
import { IconAlertTriangle, IconBulb, IconPlus } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { EmptyState } from '../../../components/common/EmptyState'
import { PageHeader } from '../../../components/common/PageHeader'
import { VirtualList } from '../../../components/common/VirtualList'
import { useIsMobile } from '../../../hooks/useIsMobile'
import { useProviders } from '../../../queries/useProvidersQueries'
import type { ApplicationList, ApplicationListQuery } from '../../../types/applications'
import { formatDateTime } from '../../../utils/formatDateTime'
import { ApplicationCards } from './ApplicationCards'
import { ApplicationRow } from './ApplicationRow'
import { ApplicationsFilters } from './ApplicationsFilters'
import { CreateApplicationModal } from './CreateApplicationModal'
import { SuggestionsDrawer } from './SuggestionsDrawer'

interface ApplicationsPageProps {
  list: ApplicationList
  query: ApplicationListQuery
  onQueryChange: (query: ApplicationListQuery) => void
}

const ROW_HEIGHT = 64

export function ApplicationsPage({ list, query, onQueryChange }: ApplicationsPageProps): ReactNode {
  const { copy, applications, config_error: configError } = list
  const { isMobile } = useIsMobile()
  const { data: providers = [] } = useProviders()
  const [suggestionsOpen, suggestions] = useDisclosure(false)
  const [creating, create] = useDisclosure(false)

  const newApplication = (
    <Button size="sm" leftSection={<IconPlus size={16} aria-hidden />} onClick={create.open}>
      {copy.new_application}
    </Button>
  )

  let content: ReactNode

  if (applications.length === 0) {
    content = <EmptyState title={copy.empty_title} description={copy.empty_description} action={newApplication} />
  } else if (isMobile) {
    content = <ApplicationCards applications={applications} copy={copy} />
  } else {
    content = (
      <VirtualList
        items={applications}
        getKey={(application) => String(application.id)}
        estimateSize={ROW_HEIGHT}
        renderItem={(application) => <ApplicationRow application={application} copy={copy} />}
      />
    )
  }

  return (
    <Stack gap="md">
      <PageHeader
        title={copy.title}
        actions={
          <Group gap="sm" wrap="nowrap">
            <Button
              size="sm"
              variant="default"
              leftSection={<IconBulb size={16} aria-hidden />}
              rightSection={<Badge size="sm">{list.suggestion_count}</Badge>}
              onClick={suggestions.open}
            >
              {copy.suggestions}
            </Button>
            {newApplication}
          </Group>
        }
      />
      {configError && (
        <Alert color="red" icon={<IconAlertTriangle size={16} aria-hidden />} title={copy.config_error_title}>
          <Text size="sm">{configError.message}</Text>
          <Text size="xs" c="dimmed" component="time" dateTime={configError.at}>
            {formatDateTime(configError.at)}
          </Text>
        </Alert>
      )}
      <ApplicationsFilters query={query} copy={copy} providers={providers} onChange={onQueryChange} />
      {content}
      <SuggestionsDrawer opened={suggestionsOpen} onClose={suggestions.close} />
      <CreateApplicationModal opened={creating} onClose={create.close} copy={copy} />
    </Stack>
  )
}
