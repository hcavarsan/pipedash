import { type ReactNode, useState } from 'react'
import { Navigate, useNavigate } from 'react-router-dom'

import { PageContainer } from '../../../components/common/PageContainer'
import { QueryBoundary } from '../../../components/common/QueryBoundary'
import { useDebounce } from '../../../hooks/useDebounce'
import { useMobileShell } from '../../../hooks/useIsMobile'
import type { ApplicationCopy } from '../../../types/applications'
import { ApplicationBoardPage } from '../components/ApplicationBoardPage'
import { useApplicationBoard, useApplications } from '../queries/useApplicationQueries'
import { DEFAULT_APPLICATION_QUERY } from '../utils/defaultApplicationQuery'

const APPLICATIONS_PATH = '/applications'

function BoardLoader({ applicationCopy }: { applicationCopy: ApplicationCopy }): ReactNode {
  const navigate = useNavigate()
  const [draft, setDraft] = useState('')
  const trimmed = useDebounce(draft).trim()
  const board = useApplicationBoard({ search: trimmed === '' ? null : trimmed })

  return (
    <QueryBoundary query={board} loading="Loading applications…" failure="Couldn't load applications">
      {(data) => (
        <ApplicationBoardPage
          board={data}
          applicationCopy={applicationCopy}
          search={draft}
          onSearchChange={setDraft}
          onBack={() => void navigate(APPLICATIONS_PATH)}
        />
      )}
    </QueryBoundary>
  )
}

export function ApplicationBoardRoute(): ReactNode {
  const mobileShell = useMobileShell()
  const list = useApplications(DEFAULT_APPLICATION_QUERY)

  if (mobileShell) {
    return <Navigate replace to={APPLICATIONS_PATH} />
  }

  return (
    <PageContainer>
      <QueryBoundary query={list} loading="Loading applications…" failure="Couldn't load applications">
        {({ copy }) => <BoardLoader applicationCopy={copy} />}
      </QueryBoundary>
    </PageContainer>
  )
}
