import { type ReactNode, useState } from 'react'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { useDebounce } from '../../../hooks/useDebounce'
import { ApplicationsPage } from '../components/ApplicationsPage'
import { useApplications } from '../queries/useApplicationQueries'
import { DEFAULT_APPLICATION_QUERY } from '../utils/defaultApplicationQuery'

export function ApplicationsRoute(): ReactNode {
  const [draft, setDraft] = useState(DEFAULT_APPLICATION_QUERY)
  const search = useDebounce(draft.search)
  const list = useApplications({ ...draft, search })

  return (
    <QueryBoundary query={list} loading="Loading applications…" failure="Couldn't load applications">
      {(data) => <ApplicationsPage list={data} query={draft} onQueryChange={setDraft} />}
    </QueryBoundary>
  )
}
