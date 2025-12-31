import { useCallback } from 'react'
import { useNavigate, useParams } from 'react-router-dom'

import { SettingsPage } from '../components/settings/SettingsPage'
import type { SettingsSection } from '../components/settings/SettingsSidebar'

const VALID_SECTIONS: SettingsSection[] = ['general', 'providers', 'metrics', 'cache', 'storage']

function isValidSection(section: string | undefined): section is SettingsSection {
  return section !== undefined && VALID_SECTIONS.includes(section as SettingsSection)
}

interface SettingsRouteProps {
  onRefresh?: () => Promise<void>
}

export function SettingsRoute({ onRefresh }: SettingsRouteProps) {
  const navigate = useNavigate()
  const { section: urlSection } = useParams<{ section?: string }>()

  const activeSection: SettingsSection = isValidSection(urlSection) ? urlSection : 'general'

  const handleSectionChange = useCallback(
    (newSection: SettingsSection) => {
      navigate(`/settings/${newSection}`, { replace: true })
    },
    [navigate]
  )

  return (
    <SettingsPage
      activeSection={activeSection}
      onSectionChange={handleSectionChange}
      onRefresh={onRefresh}
    />
  )
}
