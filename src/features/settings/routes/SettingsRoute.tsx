import type { ReactNode } from 'react'
import { Navigate, useParams } from 'react-router-dom'
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

  if (mobileShell && !isSettingsSection(section)) {
    return <SettingsMenu />
  }

  if (!isSettingsSection(section)) {
    return <Navigate to="/settings/general" replace />
  }

  return <SettingsPage activeSection={section} {...(onRefresh === undefined ? {} : { onRefresh })} />
}
