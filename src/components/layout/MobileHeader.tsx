import { ActionIcon } from '@mantine/core'
import { IconChevronLeft, IconLock, IconPlus } from '@tabler/icons-react'
import { type CSSProperties, type ReactNode, useLayoutEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { useRefresh } from '../../hooks/useRefresh'
import { useRefreshAll } from '../../queries/useRefreshQueries'
import { useLockVault, useVaultStatus } from '../../queries/useVaultQueries'
import { type BarSnapshot, useMobileNavStore } from '../../stores/mobileNavStore'
import { useProviderStore } from '../../stores/providerStore'
import { lockMeansSomething } from '../../utils/appLock'
import { ConnectionStatus } from '../common/ConnectionStatus'
import { RefreshButton } from '../common/RefreshButton'

import classes from './MobileHeader.module.css'
import { mobileChrome } from './mobileChrome'
import { type BarLayout, layoutBar } from './navigation'
import { useGoBack, useMobileBack } from './useMobileBack'

const BAR_INSET = 6
const INITIAL_LAYOUT: BarLayout = { label: 'full', labelMax: 0, side: 0 }

interface LayerProps {
  snapshot: BarSnapshot
  depth: 'deeper' | 'shallower'
  live: boolean
  layout: BarLayout
  backTo?: string
  canLock: boolean
  lock: ReactNode
  actions: ReactNode
}

function BarLayer({ snapshot, depth, live, layout, backTo, canLock, lock, actions }: LayerProps) {
  const goBack = useGoBack()
  const pushed = snapshot.backLabel !== null
  const ghostLock = (
    <span className={classes.ghost}>
      <IconLock size={22} />
    </span>
  )
  const Title = live && pushed ? 'h1' : 'div'
  const style = live ? undefined : ({ '--title-collapse': snapshot.collapse } as CSSProperties)

  return (
    <div
      className={classes.layer}
      data-depth={depth}
      data-bar-live={live ? '' : undefined}
      aria-hidden={live ? undefined : true}
      inert={!live}
      style={style}
    >
      <div className={classes.leading} data-leading>
        {pushed ? (
          live && backTo ? (
            <button
              type="button"
              className={classes.back}
              aria-label={`Back to ${snapshot.backLabel}`}
              onClick={() => goBack(backTo)}
            >
              <IconChevronLeft className={classes.chevron} aria-hidden data-chevron />
              <span className={classes.backLabel} style={{ maxWidth: layout.labelMax + 1 }}>
                {layout.label === 'short' ? 'Back' : snapshot.backLabel}
              </span>
              <span className={classes.measure} data-measure-full aria-hidden>
                {snapshot.backLabel}
              </span>
              <span className={classes.measure} data-measure-short aria-hidden>
                Back
              </span>
            </button>
          ) : (
            <span className={classes.back}>
              <IconChevronLeft className={classes.chevron} aria-hidden />
              <span className={classes.backLabel}>{snapshot.backLabel}</span>
            </span>
          )
        ) : live ? (
          lock
        ) : (
          canLock && ghostLock
        )}
      </div>

      <Title
        className={classes.title}
        data-root={pushed ? undefined : ''}
        data-page-heading={live && pushed ? '' : undefined}
        tabIndex={live && pushed ? -1 : undefined}
        aria-hidden={pushed ? undefined : true}
      >
        {snapshot.title}
      </Title>

      <div className={classes.trailing} data-trailing>
        {live ? (
          <>
            {pushed && lock}
            {actions}
          </>
        ) : (
          pushed && canLock && ghostLock
        )}
      </div>
    </div>
  )
}

export function MobileHeader() {
  useRefresh()
  const { pathname, search, state } = useLocation()
  const { title, back, tab } = mobileChrome(pathname, search, state)
  const pushedTitle = useMobileNavStore((s) => s.title)
  const transition = useMobileNavStore((s) => s.transition)
  const refreshAll = useRefreshAll()
  const lockMutation = useLockVault()
  const { data: vaultStatus } = useVaultStatus()
  const openAddProviderModal = useProviderStore((s) => s.openAddProviderModal)
  const canLock = vaultStatus ? lockMeansSomething(vaultStatus) && vaultStatus.is_unlocked : false
  const refreshable = tab === 'pipelines' || tab === 'pinned'
  const backTarget = useMobileBack()
  const barRef = useRef<HTMLDivElement>(null)
  const [layout, setLayout] = useState(INITIAL_LAYOUT)

  const live: BarSnapshot = {
    title: pushedTitle ?? title,
    backLabel: back ? (backTarget?.label ?? back.label) : null,
    collapse: 0,
  }
  const liveDeeper = transition?.kind !== 'pop'

  useLayoutEffect(() => {
    const bar = barRef.current

    if (!bar) {
      return
    }

    const measure = () => {
      const layer = bar.querySelector<HTMLElement>('[data-bar-live]')

      if (!layer) {
        return
      }

      const width = (selector: string) =>
        Math.ceil(layer.querySelector<HTMLElement>(selector)?.getBoundingClientRect().width ?? 0)
      const hasBack = layer.querySelector('[data-chevron]') !== null
      const next = layoutBar({
        bar: layer.clientWidth - BAR_INSET * 2,
        leading: hasBack ? width('[data-chevron]') : width('[data-leading]'),
        label: hasBack ? width('[data-measure-full]') : 0,
        short: width('[data-measure-short]'),
        trailing: width('[data-trailing]'),
      })

      setLayout((prev) =>
        prev.label === next.label && prev.labelMax === next.labelMax && prev.side === next.side ? prev : next
      )
    }

    const observer = new ResizeObserver(measure)

    measure()
    observer.observe(bar)
    const targets = bar.querySelectorAll('[data-leading], [data-trailing], [data-measure-full], [data-measure-short]')

    for (const element of targets) {
      observer.observe(element)
    }

    return () => observer.disconnect()
  })

  const lock = canLock ? (
    <ActionIcon aria-label="Lock Pipedash" loading={lockMutation.isPending} onClick={() => lockMutation.mutate()}>
      <IconLock size={22} />
    </ActionIcon>
  ) : null

  const actions = (
    <>
      {refreshable && (
        <RefreshButton
          iconSize={22}
          aria-label="Refresh all"
          pending={refreshAll.isPending}
          onClick={() => refreshAll.mutate()}
        />
      )}
      {tab === 'providers' && (
        <ActionIcon color="blue" aria-label="Add provider" onClick={openAddProviderModal}>
          <IconPlus size={24} />
        </ActionIcon>
      )}
    </>
  )

  const renderLayer = (snapshot: BarSnapshot, depth: 'deeper' | 'shallower', isLive: boolean) => (
    <BarLayer
      key={isLive ? 'live' : 'other'}
      snapshot={snapshot}
      depth={depth}
      live={isLive}
      layout={layout}
      backTo={backTarget?.to}
      canLock={canLock}
      lock={lock}
      actions={actions}
    />
  )

  return (
    <>
      <ConnectionStatus />
      <div
        ref={barRef}
        className={classes.bar}
        data-transition={transition ? '' : undefined}
        style={{ '--inset': `${BAR_INSET}px`, '--side': `${layout.side}px` } as CSSProperties}
      >
        <div className={classes.edge} aria-hidden />
        {transition
          ? liveDeeper
            ? [renderLayer(transition.other, 'shallower', false), renderLayer(live, 'deeper', true)]
            : [renderLayer(live, 'shallower', true), renderLayer(transition.other, 'deeper', false)]
          : renderLayer(live, back ? 'deeper' : 'shallower', true)}
      </div>
    </>
  )
}
