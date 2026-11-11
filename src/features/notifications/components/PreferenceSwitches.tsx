import { Group, Switch } from '@mantine/core'
import { type ChangeEvent, type ReactNode, useState } from 'react'
import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { SwitchRow } from '../../../components/ios/mobileParts'
import type { PreferenceChange } from '../../../types/notification_preferences'
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

function usePreferenceChange(): [boolean, (change: PreferenceChange) => void] {
  const [pending, setPending] = useState(false)
  const run = (change: PreferenceChange): void => {
    if (pending) {
      return
    }

    setPending(true)
    void changePreference(change).finally(() => setPending(false))
  }

  return [pending, run]
}

interface KindSwitchProps {
  kind: PreferenceKind
  enabled: boolean
  mobileShell: boolean
}

export function KindSwitch({ kind, enabled, mobileShell }: KindSwitchProps): ReactNode {
  const { title, description } = KINDS[kind]
  const [pending, change] = usePreferenceChange()
  const onChange = (event: ChangeEvent<HTMLInputElement>): void => {
    change({ type: 'kind', kind, enabled: event.currentTarget.checked })
  }

  return (
    <>
      <LiveStatus message={pending ? `Saving ${title}…` : ''} />
      {mobileShell ? (
        <SwitchRow title={title} subtitle={description} checked={enabled} disabled={pending} onChange={onChange} />
      ) : (
        <Switch label={title} description={description} checked={enabled} disabled={pending} onChange={onChange} />
      )}
    </>
  )
}

interface ProviderSwitchProps {
  provider: ProviderSummary
  enabled: boolean
  mobileShell: boolean
}

export function ProviderSwitch({ provider, enabled, mobileShell }: ProviderSwitchProps): ReactNode {
  const [pending, change] = usePreferenceChange()
  const onChange = (event: ChangeEvent<HTMLInputElement>): void => {
    change({ type: 'provider_muted', provider_id: provider.id, muted: !event.currentTarget.checked })
  }

  return (
    <>
      <LiveStatus message={pending ? `Saving ${provider.name}…` : ''} />
      {mobileShell ? (
        <SwitchRow title={provider.name} checked={enabled} disabled={pending} onChange={onChange} />
      ) : (
        <Switch
          label={
            <Group gap="xs" wrap="nowrap">
              <ProviderIcon name={provider.name} icon={provider.icon} size={16} />
              {provider.name}
            </Group>
          }
          checked={enabled}
          disabled={pending}
          onChange={onChange}
        />
      )}
    </>
  )
}
