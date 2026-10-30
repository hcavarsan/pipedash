import { IconChevronLeft } from '@tabler/icons-react'
import { type ReactNode, useLayoutEffect, useRef, useState } from 'react'
import { useLocation } from 'react-router-dom'

import { useRefresh } from '../../hooks/useRefresh'
import { type BarSnapshot, useMobileNavStore } from '../../stores/mobileNavStore'
import { ConnectionStatus } from '../common/ConnectionStatus'

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
  backTo?: string | undefined
}

function BarLayer({ snapshot, depth, live, layout, backTo }: LayerProps) {
  const goBack = useGoBack()
  const setNavSlot = useMobileNavStore((s) => s.setNavSlot)
  const pushed = snapshot.backLabel !== null
  const style = live ? undefined : { '--title-collapse': snapshot.collapse }

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
        {pushed &&
          (live && backTo ? (
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
          ))}
      </div>

      <div className={classes.title} aria-hidden>
        {snapshot.title}
      </div>

      <div className={classes.trailing} data-trailing>
        {live && <span className={classes.slot} ref={setNavSlot} />}
      </div>
    </div>
  )
}

export function MobileHeader(): ReactNode {
  useRefresh()
  const { pathname, search, state } = useLocation()
  const { title, back } = mobileChrome(pathname, search, state)
  const pushedTitle = useMobileNavStore((s) => s.title)
  const transition = useMobileNavStore((s) => s.transition)
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

  const renderLayer = (snapshot: BarSnapshot, depth: 'deeper' | 'shallower', isLive: boolean) => (
    <BarLayer
      key={isLive ? 'live' : 'other'}
      snapshot={snapshot}
      depth={depth}
      live={isLive}
      layout={layout}
      backTo={backTarget?.to}
    />
  )

  return (
    <>
      <ConnectionStatus />
      <div
        ref={barRef}
        className={classes.bar}
        data-transition={transition ? '' : undefined}
        style={{ '--inset': `${BAR_INSET}px`, '--side': `${layout.side}px` }}
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
