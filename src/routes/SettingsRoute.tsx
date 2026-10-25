import { useParams } from 'react-router-dom'
import { SettingsMenu } from '../components/settings/SettingsMenu'
import { SettingsPage } from '../components/settings/SettingsPage'
import { isSettingsSection } from '../components/settings/SettingsSidebar'
import { useMobileShell } from '../hooks/useIsMobile'

interface SettingsRouteProps {
  onRefresh?: () => Promise<void>
}

export function SettingsRoute({ onRefresh }: SettingsRouteProps) {
  const { section } = useParams<{ section?: string }>()
  const mobileShell = useMobileShell()

  if (mobileShell && !isSettingsSection(section)) {
    return <SettingsMenu />
  }

  return <SettingsPage activeSection={isSettingsSection(section) ? section : 'general'} onRefresh={onRefresh} />
}
