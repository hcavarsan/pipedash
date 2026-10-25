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
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

import { useMobileShell } from '@/hooks/useIsMobile'
import { logger } from '@/lib/logger'
import { queryKeys } from '@/lib/queryKeys'
import { useStorageConfig } from '@/queries/useStorageQueries'
import { service } from '@/services'
import { displayErrorNotification } from '@/utils/errorDisplay'
import { getPlatformOverride, isMobilePlatform, platform, setPlatformOverride } from '@/utils/platform'

import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { FieldRow, RowSelect, SwitchRow } from '../mobileParts'
import { useFactoryResetConfirm } from '../useFactoryResetConfirm'

interface GeneralSectionProps {
  onRefresh?: () => Promise<void>
}

const THEME_OPTIONS = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
  { value: 'auto', label: 'System' },
]

export function GeneralSection({ onRefresh }: GeneralSectionProps) {
  const mobileShell = useMobileShell()
  const [platformOverride, setPlatformOverrideState] = useState<string | null>(null)
  const [currentPlatform, setCurrentPlatform] = useState<string>('linux')
  const { colorScheme, setColorScheme } = useMantineColorScheme()
  const { confirmReset, isResetting } = useFactoryResetConfirm(async () => {
    setPlatformOverrideState(null)
    await onRefresh?.()
  })

  const queryClient = useQueryClient()
  const storageConfig = useStorageConfig()
  const general = storageConfig.data?.config.general
  const statusPagesMutation = useMutation({
    mutationFn: (enabled: boolean) => {
      if (!general) {
        throw new Error('Settings are not loaded yet')
      }

      return service.updateGeneralSettings({
        metrics_enabled: general.metrics_enabled,
        default_refresh_interval: general.default_refresh_interval,
        status_pages_enabled: enabled,
      })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update status page setting', error)
      displayErrorNotification(error, "Couldn't update status page setting")
    },
  })
  const statusPagesChecked = statusPagesMutation.isPending
    ? statusPagesMutation.variables
    : (general?.status_pages_enabled ?? false)

  useEffect(() => {
    const loadPlatform = async () => {
      const override = getPlatformOverride()

      setPlatformOverrideState(override)
      const detectedPlatform = await platform()

      setCurrentPlatform(detectedPlatform)
    }

    loadPlatform()
  }, [])

  const handlePlatformChange = (value: string | null) => {
    setPlatformOverride(value as 'macos' | 'windows' | 'linux' | null)
    setPlatformOverrideState(value)
  }

  if (mobileShell) {
    return (
      <Stack gap="lg" pb="md">
        <IosGroup footer="Choose Dark, Light, or follow your system setting.">
          <FieldRow title="Theme">
            {(id) => (
              <RowSelect
                id={id}
                data={THEME_OPTIONS}
                value={colorScheme}
                onChange={(value) => value && setColorScheme(value as MantineColorScheme)}
              />
            )}
          </FieldRow>
        </IosGroup>

        <IosGroup footer="Reads the public status page of each provider, or the one you configured. No credentials are sent.">
          <SwitchRow
            title="Check provider status pages"
            checked={statusPagesChecked}
            onChange={(e) => statusPagesMutation.mutate(e.currentTarget.checked)}
            disabled={!general}
          />
        </IosGroup>

        <IosGroup footer="Permanently delete all application data">
          <IosRow
            title="Factory reset"
            destructive
            disabled={isResetting}
            onClick={confirmReset}
            accessory={isResetting ? <Loader size={18} color="red" /> : undefined}
          />
        </IosGroup>
      </Stack>
    )
  }

  return (
    <Stack gap="md">
      <Card padding="md">
        <Stack gap="md">
          <Input.Wrapper label="Theme" description="Choose Dark, Light, or follow your system setting.">
            <SegmentedControl
              mt="xs"
              aria-label="Theme"
              fullWidth
              data={THEME_OPTIONS}
              value={colorScheme}
              onChange={(value) => setColorScheme(value as MantineColorScheme)}
            />
          </Input.Wrapper>

          <Divider />

          {!isMobilePlatform() && (
            <>
              <Select
                label="Platform"
                description={`Detected: ${currentPlatform}`}
                placeholder="Auto-detect"
                data={[
                  { value: '', label: 'Auto-detect' },
                  { value: 'macos', label: 'macOS' },
                  { value: 'windows', label: 'Windows' },
                  { value: 'linux', label: 'Linux' },
                ]}
                value={platformOverride || ''}
                onChange={handlePlatformChange}
              />

              <Divider />
            </>
          )}

          {storageConfig.isLoading ? (
            <LoadingState variant="compact" message="Loading provider status page setting…" />
          ) : storageConfig.error || !general ? (
            <ErrorState
              title="Couldn't load provider status page setting"
              error={storageConfig.error ?? 'The storage configuration did not include general settings.'}
              onRetry={() => storageConfig.refetch()}
              retrying={storageConfig.isRefetching}
            />
          ) : (
            <Switch
              label="Check provider status pages"
              description="Reads the public status page of each provider, or the one you configured. No credentials are sent."
              checked={statusPagesChecked}
              onChange={(e) => statusPagesMutation.mutate(e.currentTarget.checked)}
              disabled={statusPagesMutation.isPending}
            />
          )}
        </Stack>
      </Card>

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
            leftSection={<IconTrash size={16} />}
            variant="light"
            color="red"
            fullWidth
            onClick={confirmReset}
            loading={isResetting}
          >
            Factory reset
          </Button>
        </Stack>
      </Card>
    </Stack>
  )
}
