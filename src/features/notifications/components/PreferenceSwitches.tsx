import { Group, Switch } from '@mantine/core'
import type { ChangeEvent, ReactNode } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { SwitchRow } from '../../../components/ios/mobileParts'
import type { PreferenceKind } from '../../../types/notifications'
import type { ProviderSummary } from '../../../types/providers'
import { changePreference } from '../queries/changePreference'

const KINDS: Record<PreferenceKind, { title: string; description: string }> = {
  failed: { title: 'Failed runs', description: 'A pipeline run fails.' },
  degraded: { title: 'Degraded workloads', description: 'A deployed workload reports it is unhealthy.' },
  long_run: { title: 'Slow runs', description: 'A run takes far longer than usual.' },
  outage: { title: 'Provider outages', description: "A provider can't be reached or reports an incident." },
  recovered: { title: 'Recoveries', description: 'A failed pipeline passes again or a provider outage ends.' },
}

interface KindSwitchProps {
  kind: PreferenceKind
  enabled: boolean
  mobileShell: boolean
}

export function KindSwitch({ kind, enabled, mobileShell }: KindSwitchProps): ReactNode {
  const { title, description } = KINDS[kind]
  const onChange = (event: ChangeEvent<HTMLInputElement>): void => {
    void changePreference({ type: 'kind', kind, enabled: event.currentTarget.checked })
  }

  if (mobileShell) {
    return <SwitchRow title={title} subtitle={description} checked={enabled} onChange={onChange} />
  }

  return <Switch label={title} description={description} checked={enabled} onChange={onChange} />
}

interface ProviderSwitchProps {
  provider: ProviderSummary
  enabled: boolean
  mobileShell: boolean
}

export function ProviderSwitch({ provider, enabled, mobileShell }: ProviderSwitchProps): ReactNode {
  const onChange = (event: ChangeEvent<HTMLInputElement>): void => {
    void changePreference({
      type: 'provider_muted',
      provider_id: provider.id,
      muted: !event.currentTarget.checked,
    })
  }

  if (mobileShell) {
    return <SwitchRow title={provider.name} checked={enabled} onChange={onChange} />
  }

  return (
    <Switch
      label={
        <Group gap="xs" wrap="nowrap">
          <ProviderIcon name={provider.name} icon={provider.icon} size={16} />
          {provider.name}
        </Group>
      }
      checked={enabled}
      onChange={onChange}
    />
  )
}
