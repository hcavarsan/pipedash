import { useParams } from 'react-router-dom'

import { SettingsPage } from '../components/settings/SettingsPage'
import { isSettingsSection } from '../components/settings/SettingsSidebar'

interface SettingsRouteProps {
  onRefresh?: () => Promise<void>
}

export function SettingsRoute({ onRefresh }: SettingsRouteProps) {
  const { section } = useParams<{ section?: string }>()

  return <SettingsPage activeSection={isSettingsSection(section) ? section : 'general'} onRefresh={onRefresh} />
}
