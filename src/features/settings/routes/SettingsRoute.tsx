import { Button } from '@mantine/core'
import { IconSettings } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { Link, Navigate, useParams } from 'react-router-dom'
import { EmptyState } from '../../../components/common/EmptyState'
import { isSettingsSection } from '../../../components/common/SettingsSidebar'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { SettingsMenu } from '../components/SettingsMenu'
import { SettingsPage } from '../components/SettingsPage'

interface SettingsRouteProps {
  onRefresh?: (() => Promise<void>) | undefined
}

export function SettingsRoute({ onRefresh }: SettingsRouteProps): ReactNode {
  const { section } = useParams<{ section?: string }>()
  const mobileShell = useMobileShell()

  if (isSettingsSection(section)) {
    return <SettingsPage activeSection={section} {...(onRefresh === undefined ? {} : { onRefresh })} />
  }

  if (section !== undefined) {
    return (
      <EmptyState
        icon={<IconSettings size={24} aria-hidden />}
        title="Settings page not found"
        description="This settings address doesn't exist."
        action={
          <Button component={Link} to="/settings">
            Back to Settings
          </Button>
        }
      />
    )
  }

  return mobileShell ? <SettingsMenu /> : <Navigate to="/settings/general" replace />
}
