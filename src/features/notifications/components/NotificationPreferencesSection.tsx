import { Button, Card, Group, Stack, Switch, Text } from '@mantine/core'
import type { ReactNode } from 'react'

import { ProviderIcon } from '../../../components/atoms/ProviderIcon'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { SwitchRow } from '../../../components/ios/mobileParts'
import { useMobileShell } from '../../../hooks/useIsMobile'
import { usePipelines } from '../../../queries/usePipelinesQueries'
import { useProviders } from '../../../queries/useProvidersQueries'
import { PREFERENCE_KINDS, useNotificationStore } from '../../../stores/notificationStore'
import type { PreferenceKind } from '../../../types/domain'

const KINDS: Record<PreferenceKind, { title: string; description: string }> = {
  failed: { title: 'Failed runs', description: 'A pipeline run fails.' },
  degraded: { title: 'Degraded workloads', description: 'A deployed workload reports it is unhealthy.' },
  long_run: { title: 'Slow runs', description: 'A run takes far longer than usual.' },
  outage: { title: 'Provider outages', description: "A provider can't be reached or reports an incident." },
  recovered: { title: 'Recoveries', description: 'A provider outage ends.' },
}

const KINDS_NOTE =
  'Applies to notifications and the notification center on this device only. Each phone, computer and browser keeps its own choices.'
const PROVIDERS_NOTE = 'Turn off a provider to stop all of its notifications.'
const MUTED_EMPTY = 'To mute a pipeline, open its menu and choose Mute notifications.'

interface NotificationPreferencesSectionProps {
  hostNote?: string | undefined
}

function SectionHeading({ title, description }: { title: string; description: string }): ReactNode {
  return (
    <Stack gap={0}>
      <Text size="sm" fw={600}>
        {title}
      </Text>
      <Text size="xs" c="dimmed">
        {description}
      </Text>
    </Stack>
  )
}

export function NotificationPreferencesSection({ hostNote }: NotificationPreferencesSectionProps): ReactNode {
  const mobileShell = useMobileShell()
  const preferences = useNotificationStore((state) => state.preferences)
  const setKindEnabled = useNotificationStore((state) => state.setKindEnabled)
  const setProviderMuted = useNotificationStore((state) => state.setProviderMuted)
  const setPipelineMuted = useNotificationStore((state) => state.setPipelineMuted)
  const { data: providers = [] } = useProviders()
  const { data: pipelines = [] } = usePipelines()

  const muted = preferences.muted_pipelines.map((id) => {
    const pipeline = pipelines.find((item) => item.id === id)
    const provider = providers.find((item) => item.id === pipeline?.provider_id)

    return {
      id,
      name: pipeline?.name ?? id,
      detail: pipeline
        ? [pipeline.repository, provider?.name ?? pipeline.provider_type].filter(Boolean).join(' · ')
        : 'No longer in Pipedash',
    }
  })
  const kindsNote = hostNote ? `${KINDS_NOTE} ${hostNote}` : KINDS_NOTE

  if (mobileShell) {
    return (
      <>
        <IosGroup header="Notify me about" footer={kindsNote}>
          {PREFERENCE_KINDS.map((kind) => (
            <SwitchRow
              key={kind}
              title={KINDS[kind].title}
              subtitle={KINDS[kind].description}
              checked={preferences[kind]}
              onChange={(event) => setKindEnabled(kind, event.currentTarget.checked)}
            />
          ))}
        </IosGroup>

        {providers.length > 0 && (
          <IosGroup header="Providers" footer={PROVIDERS_NOTE}>
            {providers.map((provider) => (
              <SwitchRow
                key={provider.id}
                title={provider.name}
                checked={!preferences.muted_providers.includes(provider.id)}
                onChange={(event) => setProviderMuted(provider.id, !event.currentTarget.checked)}
              />
            ))}
          </IosGroup>
        )}

        <IosGroup header="Muted pipelines" footer={muted.length === 0 ? MUTED_EMPTY : undefined}>
          {muted.length === 0 ? (
            <IosRow
              title={
                <Text span inherit c="dimmed">
                  None
                </Text>
              }
            />
          ) : (
            muted.map((pipeline) => (
              <IosRow
                key={pipeline.id}
                title={pipeline.name}
                subtitle={pipeline.detail}
                accessory={
                  <Button
                    variant="subtle"
                    size="compact-sm"
                    aria-label={`Unmute ${pipeline.name}`}
                    onClick={() => setPipelineMuted(pipeline.id, false)}
                  >
                    Unmute
                  </Button>
                }
              />
            ))
          )}
        </IosGroup>
      </>
    )
  }

  return (
    <>
      <Card padding="md">
        <Stack gap="md">
          <SectionHeading title="Notify me about" description={kindsNote} />
          {PREFERENCE_KINDS.map((kind) => (
            <Switch
              key={kind}
              label={KINDS[kind].title}
              description={KINDS[kind].description}
              checked={preferences[kind]}
              onChange={(event) => setKindEnabled(kind, event.currentTarget.checked)}
            />
          ))}
        </Stack>
      </Card>

      {providers.length > 0 && (
        <Card padding="md">
          <Stack gap="md">
            <SectionHeading title="Providers" description={PROVIDERS_NOTE} />
            {providers.map((provider) => (
              <Switch
                key={provider.id}
                label={
                  <Group gap="xs" wrap="nowrap">
                    <ProviderIcon name={provider.name} icon={provider.icon} size={16} />
                    {provider.name}
                  </Group>
                }
                checked={!preferences.muted_providers.includes(provider.id)}
                onChange={(event) => setProviderMuted(provider.id, !event.currentTarget.checked)}
              />
            ))}
          </Stack>
        </Card>
      )}

      <Card padding="md">
        <Stack gap="md">
          <SectionHeading
            title="Muted pipelines"
            description={muted.length === 0 ? MUTED_EMPTY : 'These pipelines never notify you.'}
          />
          {muted.map((pipeline) => (
            <Group key={pipeline.id} justify="space-between" wrap="nowrap" gap="sm">
              <Stack gap={0} miw={0}>
                <Text size="sm" truncate>
                  {pipeline.name}
                </Text>
                <Text size="xs" c="dimmed" truncate>
                  {pipeline.detail}
                </Text>
              </Stack>
              <Button
                variant="default"
                size="compact-sm"
                aria-label={`Unmute ${pipeline.name}`}
                onClick={() => setPipelineMuted(pipeline.id, false)}
              >
                Unmute
              </Button>
            </Group>
          ))}
        </Stack>
      </Card>
    </>
  )
}
