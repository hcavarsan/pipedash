import { ActionIcon } from '@mantine/core'
import { IconChevronLeft, IconLock, IconPlus, IconRefresh } from '@tabler/icons-react'
import { Link, useLocation } from 'react-router-dom'

import { useRefresh } from '../../hooks/useRefresh'
import { useScrolledPast } from '../../hooks/useScrolledPast'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { useMobileNavStore } from '../../stores/mobileNavStore'
import { useProviderStore } from '../../stores/providerStore'
import { lockMeansSomething } from '../../utils/appLock'
import { ConnectionStatus } from '../common/ConnectionStatus'

import classes from './MobileHeader.module.css'
import { mobileChrome } from './mobileChrome'
import { useMobileBack } from './useMobileBack'

const LARGE_TITLE_COLLAPSE = 36

export function MobileHeader() {
  useRefresh()
  const { pathname, search } = useLocation()
  const { title, back, tab } = mobileChrome(pathname, search)
  const pushedTitle = useMobileNavStore((s) => s.title)
  const scrolled = useScrolledPast(LARGE_TITLE_COLLAPSE)
  const refreshAll = useRefreshAll()
  const lockMutation = useLockVault()
  const { data: vaultStatus } = useVaultStatus()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const canLock = vaultStatus ? lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked : false
  const refreshable = tab === 'pipelines' || tab === 'pinned'
  const backTarget = useMobileBack()

  return (
    <>
      <ConnectionStatus />
      <div className={classes.bar} data-scrolled={scrolled || back ? '' : undefined}>
        <div className={classes.side}>
          {backTarget && (
            <Link to={backTarget.to} className={classes.back} aria-label={`Back to ${backTarget.label}`}>
              <IconChevronLeft size={26} aria-hidden />
              <span>{backTarget.label}</span>
            </Link>
          )}
          {!back && canLock && (
            <ActionIcon
              variant="subtle"
              color="gray"
              aria-label="Lock Pipedash"
              loading={lockMutation.isPending}
              onClick={() => lockMutation.mutate()}
            >
              <IconLock size={22} />
            </ActionIcon>
          )}
        </div>

        <div className={classes.title} data-visible={scrolled || back ? '' : undefined}>
          {pushedTitle ?? title}
        </div>

        <div className={`${classes.side} ${classes.sideEnd}`}>
          {back && canLock && (
            <ActionIcon
              variant="subtle"
              color="gray"
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
              aria-label="Refresh all"
              loading={refreshAll.isPending}
              onClick={() => refreshAll.mutate()}
            >
              <IconRefresh size={22} />
            </ActionIcon>
          )}
          {tab === 'providers' && (
            <ActionIcon variant="subtle" color="blue" aria-label="Add provider" onClick={openAddProviderModal}>
              <IconPlus size={24} />
            </ActionIcon>
          )}
        </div>
      </div>
    </>
  )
}
