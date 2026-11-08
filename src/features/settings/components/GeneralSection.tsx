import {
  Button,
  Card,
  Divider,
  Input,
  Loader,
  type MantineColorScheme,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Text,
  useMantineColorScheme,
} from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type ComponentProps, type ReactNode, useState } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { useStorageConfig } from '@/queries/useStorageQueries'
import { isRemoteClient } from '@/services/connection'
import { service } from '@/services/service'
import type { GeneralConfig } from '@/types/storage'
import { displayErrorNotification } from '@/utils/errorDisplay'
import { isOneOf } from '@/utils/isOneOf'
import { getPlatformOverride, isDesktopTauri, isMobilePlatform, platform, setPlatformOverride } from '@/utils/platform'

import { ErrorState } from '../../../components/common/ErrorState'
import { LoadingState } from '../../../components/common/LoadingState'
import { LiveStatus } from '../../../components/forms/LiveStatus'
import { IosGroup, IosRow } from '../../../components/ios/IosList'
import { FieldRow, RowSelect, SwitchRow } from '../../../components/ios/mobileParts'
import { useFactoryResetConfirm } from '../hooks/useFactoryResetConfirm'

import { DesktopIntegrationCard } from './DesktopIntegrationCard'
import { ProxySettingsForm } from './ProxySettingsForm'

interface GeneralSectionProps {
  onRefresh?: (() => Promise<void>) | undefined
}

const THEME_OPTIONS: { value: MantineColorScheme; label: string }[] = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
  { value: 'auto', label: 'System' },
]

const THEME_VALUES = THEME_OPTIONS.map((option) => option.value)

type PlatformChoice = '' | 'macos' | 'windows' | 'linux'

const PLATFORM_OPTIONS: { value: PlatformChoice; label: string }[] = [
  { value: '', label: 'Auto-detect' },
  { value: 'macos', label: 'macOS' },
  { value: 'windows', label: 'Windows' },
  { value: 'linux', label: 'Linux' },
]

interface FactoryResetCardProps {
  onReset: () => void
  resetting: boolean
}

function FactoryResetCard({ onReset, resetting }: FactoryResetCardProps): ReactNode {
  return (
    <Card padding="md">
      <Stack gap="md">
        <Stack gap={0}>
          <Text size="sm" fw={600} c="red">
            Danger zone
          </Text>
          <Text size="xs" c="dimmed">
            Permanently delete all application data
          </Text>
        </Stack>

        <Divider />

        <Button
          leftSection={<IconTrash size={16} aria-hidden />}
          variant="light"
          color="red"
          fullWidth
          onClick={onReset}
          loading={resetting}
        >
          Factory reset
        </Button>
      </Stack>
    </Card>
  )
}

export function GeneralSection({ onRefresh }: GeneralSectionProps): ReactNode {
  const mobileShell = useMobileShell()
  const queryClient = useQueryClient()
  const [platformOverride, setPlatformOverrideState] = useState(getPlatformOverride)
  const platformQuery = useQuery({ queryKey: queryKeys.platform.current(), queryFn: platform })
  const currentPlatform = platformQuery.data ?? 'linux'
  const { colorScheme, setColorScheme } = useMantineColorScheme()
  const { confirmReset, isResetting } = useFactoryResetConfirm(async () => {
    setPlatformOverrideState(null)
    void queryClient.invalidateQueries({ queryKey: queryKeys.platform.current() })
    await onRefresh?.()
  })

  const storageConfig = useStorageConfig()
  const general = storageConfig.data?.config.general
  const generalMutation = useMutation({
    mutationFn: (update: GeneralConfig) => service.updateGeneralSettings(update),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update general settings', error)
      displayErrorNotification(error, "Couldn't save general settings")
    },
  })
  const setStatusPages = (checked: boolean): void => {
    if (general) {
      generalMutation.mutate({ ...general, status_pages_enabled: checked })
    }
  }
  const statusPagesChecked = generalMutation.isPending
    ? generalMutation.variables.status_pages_enabled
    : (general?.status_pages_enabled ?? false)
  let statusPagesControl: ReactNode
  if (storageConfig.isLoading) {
    statusPagesControl = <LoadingState variant="compact" message="Loading provider status page setting…" />
  } else if (storageConfig.error || !general) {
    statusPagesControl = (
      <ErrorState
        title="Couldn't load provider status page setting"
        error={storageConfig.error ?? 'The storage configuration did not include general settings.'}
        onRetry={() => void storageConfig.refetch()}
        retrying={storageConfig.isRefetching}
      />
    )
  } else if (mobileShell) {
    statusPagesControl = (
      <SwitchRow
        title="Check provider status pages"
        checked={statusPagesChecked}
        onChange={(event) => setStatusPages(event.currentTarget.checked)}
        disabled={generalMutation.isPending}
      />
    )
  } else {
    statusPagesControl = (
      <Switch
        label="Check provider status pages"
        description="Reads the public status page of each provider, or the one you configured. No credentials are sent."
        checked={statusPagesChecked}
        onChange={(event) => setStatusPages(event.currentTarget.checked)}
        disabled={generalMutation.isPending}
      />
    )
  }

  const proxyForm = general && (
    <ProxySettingsForm
      savedProxy={general.proxy_url ?? ''}
      mobileShell={mobileShell}
      pending={generalMutation.isPending}
      onSave={(proxyUrl) => generalMutation.mutateAsync({ ...general, proxy_url: proxyUrl })}
    />
  )

  const handlePlatformChange = (value: PlatformChoice | null): void => {
    setPlatformOverride(value || null)
    setPlatformOverrideState(value || null)
    void queryClient.invalidateQueries({ queryKey: queryKeys.platform.current() })
  }

  const handleThemeChange: ComponentProps<typeof RowSelect>['onChange'] = (value) => {
    if (isOneOf(THEME_VALUES, value)) {
      setColorScheme(value)
    }
  }

  const renderThemeSelect = (id: string): ReactNode => (
    <RowSelect id={id} data={THEME_OPTIONS} value={colorScheme} onChange={handleThemeChange} />
  )

  const platformSection = isMobilePlatform() ? null : (
    <>
      <Select<PlatformChoice>
        label="Platform"
        description={`Detected: ${currentPlatform}`}
        placeholder="Auto-detect"
        data={PLATFORM_OPTIONS}
        value={PLATFORM_OPTIONS.find((option) => option.value === platformOverride)?.value ?? ''}
        onChange={handlePlatformChange}
      />

      <Divider />
    </>
  )

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <LiveStatus message={isResetting ? 'Resetting application data' : ''} />

        <IosGroup footer="Choose Dark, Light, or follow your system setting.">
          <FieldRow title="Theme">{renderThemeSelect}</FieldRow>
        </IosGroup>

        <IosGroup footer="Reads the public status page of each provider, or the one you configured. No credentials are sent.">
          {statusPagesControl}
        </IosGroup>

        {proxyForm}

        {!isRemoteClient() && (
          <IosGroup footer="Permanently delete all application data">
            <IosRow
              title="Factory reset"
              destructive
              disabled={isResetting}
              onClick={confirmReset}
              accessory={isResetting ? <Loader size={18} color="red" aria-hidden /> : undefined}
            />
          </IosGroup>
        )}
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      <LiveStatus message={isResetting ? 'Resetting application data' : ''} />

      <Card padding="md">
        <Stack gap="md">
          <Input.Wrapper label="Theme" description="Choose Dark, Light, or follow your system setting.">
            <SegmentedControl<MantineColorScheme>
              mt="xs"
              aria-label="Theme"
              fullWidth
              data={THEME_OPTIONS}
              value={colorScheme}
              onChange={setColorScheme}
            />
          </Input.Wrapper>

          <Divider />

          {platformSection}

          {statusPagesControl}

          {proxyForm && (
            <>
              <Divider />
              {proxyForm}
            </>
          )}
        </Stack>
      </Card>

      {isDesktopTauri() && <DesktopIntegrationCard />}

      {!isRemoteClient() && <FactoryResetCard onReset={confirmReset} resetting={isResetting} />}
    </Stack>
  )
}
