import { Button } from '@mantine/core'
import type { ReactNode } from 'react'
import { Navigate, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'

import { EmptyState } from '../../../components/common/EmptyState'
import { PageContainer } from '../../../components/common/PageContainer'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { ErrorBoundary } from '../../../components/ErrorBoundary/ErrorBoundary'
import { ApplicationDetailPage, type DetailTab } from '../components/ApplicationDetailPage'
import { useApplication } from '../queries/useApplicationQueries'

const APPLICATIONS_PATH = '/applications'

const parseTab = (raw: string | null): DetailTab | null => {
  if (raw === 'changes' || raw === 'metrics' || raw === 'flow') {
    return raw
  }

  return null
}

function ApplicationDetailLoader({ id, tab }: { id: number; tab: DetailTab }): ReactNode {
  const query = useApplication(id)
  const [, setSearchParams] = useSearchParams()
  const { state } = useLocation()
  const navigate = useNavigate()

  const setTab = (next: DetailTab): void => {
    setSearchParams(
      (previous) => {
        const params = new URLSearchParams(previous)

        if (next === 'flow') {
          params.delete('tab')
        } else {
          params.set('tab', next)
        }

        return params
      },
      { replace: true, state }
    )
  }

  return (
    <ErrorBoundary scope="feature" name="Application" resetKeys={[id]}>
      <PageContainer>
        <QueryBoundary query={query} loading="Loading application…" failure="Couldn't load this application">
          {(detail) => (
            <ApplicationDetailPage
              detail={detail}
              tab={tab}
              onTabChange={setTab}
              onBack={() => void navigate(APPLICATIONS_PATH)}
            />
          )}
        </QueryBoundary>
      </PageContainer>
    </ErrorBoundary>
  )
}

export function ApplicationDetailRoute(): ReactNode {
  const { applicationId } = useParams()
  const [searchParams] = useSearchParams()
  const { state, pathname } = useLocation()
  const navigate = useNavigate()
  const rawTab = searchParams.get('tab')
  const tab = parseTab(rawTab)
  const id = Number(applicationId)

  if (rawTab !== null && tab === null) {
    const params = new URLSearchParams(searchParams)
    params.delete('tab')

    return <Navigate replace state={state} to={{ pathname, search: params.toString() }} />
  }

  if (!Number.isInteger(id) || id <= 0) {
    return (
      <PageContainer>
        <EmptyState
          title="Couldn't find this application"
          description="It may have been removed."
          action={
            <Button variant="default" onClick={() => void navigate(APPLICATIONS_PATH)}>
              Back
            </Button>
          }
        />
      </PageContainer>
    )
  }

  return <ApplicationDetailLoader id={id} tab={tab ?? 'flow'} />
}
