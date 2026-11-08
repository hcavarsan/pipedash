import { Button } from '@mantine/core'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'

import { EmptyState } from './EmptyState'
import { PageContainer } from './PageContainer'
import { PageHeader } from './PageHeader'

export function NotFoundPage(): ReactNode {
  const navigate = useNavigate()

  return (
    <PageContainer>
      <PageHeader title="Page not found" />
      <EmptyState
        title="This page doesn't exist"
        description="Check the address or return to your pipelines."
        action={
          <Button
            variant="default"
            onClick={() => {
              void navigate('/pipelines')
            }}
          >
            Pipelines
          </Button>
        }
      />
    </PageContainer>
  )
}
