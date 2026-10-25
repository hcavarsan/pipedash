import {
  Button,
  Card,
  Divider,
  Group,
  Input,
  Loader,
  type MantineColorScheme,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
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
import type { GeneralConfig } from '@/types'
import { displayErrorNotification } from '@/utils/errorDisplay'
import { isHttpUrl } from '@/utils/openExternal'
import { getPlatformOverride, isMobilePlatform, platform, setPlatformOverride } from '@/utils/platform'

import { ErrorState } from '../../common/ErrorState'
import { LoadingState } from '../../common/LoadingState'
import { IosGroup, IosRow } from '../../ios/IosList'
import { AccentText, FieldRow, RowSelect, RowTextInput, SwitchRow } from '../mobileParts'
import { useFactoryResetConfirm } from '../useFactoryResetConfirm'

interface GeneralSectionProps {
  onRefresh?: () => Promise<void>
}

const THEME_OPTIONS = [
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
  { value: 'auto', label: 'System' },
]

const PROXY_DESCRIPTION =
  "Sends provider requests through this HTTP proxy, except Tekton and Flux. Leave it empty to connect directly or through HTTPS_PROXY. Proxies that need a password aren't supported."

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
  const [proxyDraft, setProxyDraft] = useState<string | null>(null)
  const generalMutation = useMutation({
    mutationFn: (update: GeneralConfig) => service.updateGeneralSettings(update),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.storage.config() }),
    onError: (error) => {
      logger.error('GeneralSection', 'Failed to update general settings', error)
      displayErrorNotification(error, "Couldn't save general settings")
    },
  })
  const saveGeneral = (change: Partial<GeneralConfig>, onSuccess?: () => void) => {
    if (!general) {
      return
    }

    generalMutation.mutate({ ...general, ...change }, { onSuccess })
  }
  const statusPagesChecked = generalMutation.isPending
    ? generalMutation.variables.status_pages_enabled
    : (general?.status_pages_enabled ?? false)
  const statusPagesControl = storageConfig.isLoading ? (
    <LoadingState variant="compact" message="Loading provider status page setting…" />
  ) : storageConfig.error || !general ? (
    <ErrorState
      title="Couldn't load provider status page setting"
      error={storageConfig.error ?? 'The storage configuration did not include general settings.'}
      onRetry={() => storageConfig.refetch()}
      retrying={storageConfig.isRefetching}
    />
  ) : mobileShell ? (
    <SwitchRow
      title="Check provider status pages"
      checked={statusPagesChecked}
      onChange={(e) => saveGeneral({ status_pages_enabled: e.currentTarget.checked })}
      disabled={generalMutation.isPending}
    />
  ) : (
    <Switch
      label="Check provider status pages"
      description="Reads the public status page of each provider, or the one you configured. No credentials are sent."
      checked={statusPagesChecked}
      onChange={(e) => saveGeneral({ status_pages_enabled: e.currentTarget.checked })}
      disabled={generalMutation.isPending}
    />
  )

  const savedProxy = general?.proxy_url ?? ''
  const proxyValue = proxyDraft ?? savedProxy
  const proxyTrimmed = proxyValue.trim()
  const proxyError =
    proxyTrimmed && !isHttpUrl(proxyTrimmed) ? 'Enter a URL that starts with http:// or https://' : undefined
  const canSaveProxy = !!general && !generalMutation.isPending && !proxyError && proxyTrimmed !== savedProxy
  const saveProxy = () => {
    if (canSaveProxy) {
      saveGeneral({ proxy_url: proxyTrimmed }, () => setProxyDraft(null))
    }
  }

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
          {statusPagesControl}
        </IosGroup>

        {general && (
          <>
            <IosGroup footer={proxyError ?? PROXY_DESCRIPTION}>
              <FieldRow title="Proxy URL">
                {(id) => (
                  <RowTextInput
                    id={id}
                    placeholder="None"
                    type="url"
                    inputMode="url"
                    autoComplete="off"
                    enterKeyHint="done"
                    value={proxyValue}
                    onChange={(e) => setProxyDraft(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        e.preventDefault()
                        saveProxy()
                      }
                    }}
                  />
                )}
              </FieldRow>
            </IosGroup>

            <IosGroup>
              <IosRow
                title={<AccentText>{generalMutation.isPending ? 'Saving...' : 'Save proxy'}</AccentText>}
                disabled={!canSaveProxy}
                onClick={saveProxy}
              />
            </IosGroup>
          </>
        )}

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

          {statusPagesControl}

          {general && (
            <>
              <Divider />

              <form
                noValidate
                onSubmit={(event) => {
                  event.preventDefault()
                  saveProxy()
                }}
              >
                <Stack gap="xs">
                  <TextInput
                    label="Proxy URL"
                    description={PROXY_DESCRIPTION}
                    placeholder="http://proxy.example.com:3128"
                    type="url"
                    inputMode="url"
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    value={proxyValue}
                    onChange={(e) => setProxyDraft(e.currentTarget.value)}
                    error={proxyError}
                    disabled={generalMutation.isPending}
                  />
                  <Group justify="flex-end">
                    <Button type="submit" size="xs" disabled={!canSaveProxy} loading={generalMutation.isPending}>
                      Save proxy
                    </Button>
                  </Group>
                </Stack>
              </form>
            </>
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
