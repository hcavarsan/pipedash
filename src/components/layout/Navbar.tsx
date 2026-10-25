import {
  ActionIcon,
  Box,
  Button,
  Center,
  Divider,
  Group,
  Loader,
  NavLink,
  ScrollArea,
  Stack,
  Text,
  ThemeIcon,
  Tooltip,
} from '@mantine/core'
import { IconAlertTriangle, IconEdit, IconPinFilled, IconPlus, IconStack2, IconTrash } from '@tabler/icons-react'
import { Link, useLocation, useSearchParams } from 'react-router-dom'

import { useProviders } from '../../queries/useProvidersQueries'
import { useProviderStore } from '../../stores/providerStore'
import type { ProviderSummary } from '../../types'
import { formatCount } from '../../utils/formatCount'
import { formatTime } from '../../utils/formatDateTime'
import { ProviderIcon } from '../atoms/ProviderIcon'
import { EmptyState } from '../common/EmptyState'
import { ErrorState } from '../common/ErrorState'
import { LoadingState } from '../common/LoadingState'
import { useConfirmRemoveProvider } from '../provider/ProviderModals'

interface NavbarProps {
  compact?: boolean
}

const providerStatus = (provider: ProviderSummary) => {
  if (provider.paused_until) {
    return { text: `Paused until ${formatTime(provider.paused_until)}`, failed: false }
  }
  if (provider.last_fetch_status === 'error') {
    return { text: 'Fetch failed', failed: true }
  }

  return { text: formatCount(provider.pipeline_count, 'pipeline'), failed: false }
}

const ICON_TILE = { rail: 30, list: 28 } as const

const NavTile = ({ icon: Icon, size }: { icon: typeof IconStack2; size: number }) => (
  <ThemeIcon size={size} variant="transparent" color="gray" aria-hidden>
    <Icon size={size * 0.6} />
  </ThemeIcon>
)

export const ProviderAvatar = ({ provider, size }: { provider: ProviderSummary; size: number }) => (
  <ProviderIcon
    name={provider.name}
    icon={provider.icon}
    size={size}
    attention={provider.last_fetch_status === 'error' ? { tone: 'danger', label: 'Fetch failed' } : undefined}
  />
)

export const ProviderStatusLine = ({ provider }: { provider: ProviderSummary }) => {
  const { text, failed } = providerStatus(provider)

  return (
    <Text size="xs" c={failed ? 'red' : 'dimmed'} truncate>
      {text}
    </Text>
  )
}

export const Navbar = ({ compact = false }: NavbarProps) => {
  const { pathname } = useLocation()
  const [searchParams] = useSearchParams()
  const pinnedActive = pathname === '/pipelines' && searchParams.get('view') === 'pinned'
  const providerParam = pathname === '/pipelines' && !pinnedActive ? searchParams.get('provider') : null

  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const requestEditProvider = useProviderStore((s) => s.requestEditProvider)

  const {
    data: providers = [],
    isLoading: loading,
    error: providersError,
    refetch: refetchProviders,
    isRefetching: refetchingProviders,
  } = useProviders()

  const confirmRemove = useConfirmRemoveProvider()

  const totalPipelines = providers.reduce((sum, p) => sum + p.pipeline_count, 0)
  const allActive = pathname === '/pipelines' && providerParam === null && !pinnedActive

  const railItem = (label: string, to: string, active: boolean, icon: React.ReactNode) => (
    <Tooltip label={label} position="right" key={to}>
      <ActionIcon
        component={Link}
        to={to}
        size={44}
        radius="md"
        variant={active ? 'light' : 'subtle'}
        color={active ? 'blue' : 'gray'}
        aria-label={label}
        aria-current={active ? 'page' : undefined}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  )

  const content = compact ? (
    <ScrollArea flex={1} py="sm" scrollbarSize={4}>
      <Stack gap={6} align="center">
        {railItem(
          `All providers · ${formatCount(totalPipelines, 'pipeline')}`,
          '/pipelines',
          allActive,
          <NavTile icon={IconStack2} size={ICON_TILE.rail} />
        )}
        {railItem(
          'Pinned',
          '/pipelines?view=pinned',
          pinnedActive,
          <NavTile icon={IconPinFilled} size={ICON_TILE.rail} />
        )}
        <Divider w={32} my={4} />
        {loading ? (
          <Center h={44}>
            <Loader size="sm" />
          </Center>
        ) : providersError ? (
          <Tooltip label="Couldn't load providers. Click to retry." position="right">
            <ActionIcon
              size={44}
              radius="md"
              variant="light"
              color="red"
              aria-label="Couldn't load providers. Retry"
              loading={refetchingProviders}
              onClick={() => refetchProviders()}
            >
              <IconAlertTriangle size={20} />
            </ActionIcon>
          </Tooltip>
        ) : (
          providers.map((provider) =>
            railItem(
              `${provider.name} · ${providerStatus(provider).text}`,
              `/pipelines?provider=${provider.id}`,
              providerParam === String(provider.id),
              <ProviderAvatar provider={provider} size={ICON_TILE.rail} />
            )
          )
        )}
      </Stack>
    </ScrollArea>
  ) : (
    <ScrollArea flex={1} px="sm" py="sm">
      <Stack gap={2}>
        <NavLink
          leftSection={<NavTile icon={IconStack2} size={ICON_TILE.list} />}
          label="All providers"
          description={loading || providersError ? undefined : `${formatCount(totalPipelines, 'pipeline')} total`}
          active={allActive}
          component={Link}
          to="/pipelines"
          variant="light"
        />
        <NavLink
          leftSection={<NavTile icon={IconPinFilled} size={ICON_TILE.list} />}
          label="Pinned"
          active={pinnedActive}
          component={Link}
          to="/pipelines?view=pinned"
          variant="light"
        />

        <Divider my={6} />

        {loading ? (
          <LoadingState message="Loading providers" />
        ) : providersError ? (
          <ErrorState
            title="Couldn't load providers"
            error={providersError}
            hint="Check your connection and provider credentials."
            onRetry={() => refetchProviders()}
            retrying={refetchingProviders}
          />
        ) : providers.length === 0 ? (
          <EmptyState title="No providers yet" description="Add a provider to see its pipelines here." />
        ) : (
          providers.map((provider) => (
            <Group
              key={provider.id}
              gap={0}
              wrap="nowrap"
              className="nav-item"
              mod={{ active: providerParam === String(provider.id) }}
            >
              <NavLink
                flex={1}
                miw={0}
                className="nav-item-link"
                leftSection={<ProviderAvatar provider={provider} size={ICON_TILE.list} />}
                label={
                  <Text size="sm" truncate>
                    {provider.name}
                  </Text>
                }
                description={<ProviderStatusLine provider={provider} />}
                active={providerParam === String(provider.id)}
                component={Link}
                to={`/pipelines?provider=${provider.id}`}
                variant="light"
              />
              <Group gap={2} wrap="nowrap" className="nav-item-actions">
                <Tooltip label={`Edit ${provider.name}`}>
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="gray"
                    aria-label={`Edit ${provider.name}`}
                    onClick={() => requestEditProvider(provider.id)}
                  >
                    <IconEdit size={14} />
                  </ActionIcon>
                </Tooltip>
                <Tooltip label={`Remove ${provider.name}`}>
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="gray"
                    aria-label={`Remove ${provider.name}`}
                    onClick={() => confirmRemove(provider.id, provider.name)}
                  >
                    <IconTrash size={14} />
                  </ActionIcon>
                </Tooltip>
              </Group>
            </Group>
          ))
        )}
      </Stack>
    </ScrollArea>
  )

  return (
    <Stack h="100%" gap={0}>
      {content}

      <Divider />
      <Box p={compact ? 'sm' : 'md'} pb={`calc(var(--mantine-spacing-${compact ? 'sm' : 'md'}) + var(--safe-bottom))`}>
        {compact ? (
          <Center>
            <Tooltip label="Add provider" position="right">
              <ActionIcon
                size={44}
                radius="md"
                variant="default"
                aria-label="Add provider"
                onClick={openAddProviderModal}
              >
                <IconPlus size={20} />
              </ActionIcon>
            </Tooltip>
          </Center>
        ) : (
          <Button
            leftSection={<IconPlus size={16} />}
            variant="default"
            size="sm"
            fullWidth
            onClick={openAddProviderModal}
          >
            Add provider
          </Button>
        )}
      </Box>
    </Stack>
  )
}
