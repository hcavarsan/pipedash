import { ActionIcon, Box, Group, Title, UnstyledButton } from '@mantine/core'
import { IconChevronLeft, IconLock, IconPlus, IconRefresh } from '@tabler/icons-react'
import { Link, useLocation } from 'react-router-dom'

import { useRefresh } from '../../hooks/useRefresh'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { useProviderStore } from '../../stores/providerStore'
import { lockMeansSomething } from '../../utils/appLock'
import { ConnectionStatus } from '../common/ConnectionStatus'

import { mobileChrome } from './mobileChrome'

export function MobileHeader() {
  useRefresh()
  const { pathname, search } = useLocation()
  const { title, back, tab } = mobileChrome(pathname, search)
  const refreshAll = useRefreshAll()
  const lockMutation = useLockVault()
  const { data: vaultStatus } = useVaultStatus()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const canLock = vaultStatus ? lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked : false
  const refreshable = tab === 'pipelines' || tab === 'pinned'

  return (
    <>
      <ConnectionStatus />
      <Group h="100%" wrap="nowrap" justify="space-between" gap="xs" px="sm" pt="var(--safe-top)">
        <Group gap={0} wrap="nowrap" flex={1} miw={0}>
          {back ? (
            <UnstyledButton
              component={Link}
              to={back.to}
              mih={44}
              c="blue"
              aria-label={`Back to ${back.label}`}
              style={{ display: 'flex', alignItems: 'center' }}
            >
              <IconChevronLeft size={24} aria-hidden />
              <Box fz="md">{back.label}</Box>
            </UnstyledButton>
          ) : (
            <Title order={2} fz={26} fw={700} lh={1.2} lineClamp={1} pl="xs">
              {title}
            </Title>
          )}
        </Group>

        {back && (
          <Title order={3} fz="md" fw={600} lh={1.2} ta="center" lineClamp={1} maw="40%">
            {title}
          </Title>
        )}

        <Group gap={0} wrap="nowrap" flex={1} miw={0} justify="flex-end">
          {canLock && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size={44}
              aria-label="Lock Pipedash"
              loading={lockMutation.isPending}
              onClick={() => lockMutation.mutate()}
            >
              <IconLock size={22} />
            </ActionIcon>
          )}
          {refreshable && (
            <ActionIcon
              variant="subtle"
              color="gray"
              size={44}
              aria-label="Refresh all"
              loading={refreshAll.isPending}
              onClick={() => refreshAll.mutate()}
            >
              <IconRefresh size={22} />
            </ActionIcon>
          )}
          {tab === 'providers' && (
            <ActionIcon
              variant="subtle"
              color="blue"
              size={44}
              aria-label="Add provider"
              onClick={openAddProviderModal}
            >
              <IconPlus size={24} />
            </ActionIcon>
          )}
        </Group>
      </Group>
    </>
  )
}
