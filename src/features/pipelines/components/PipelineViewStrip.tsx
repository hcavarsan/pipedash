import { type ReactNode, useLayoutEffect, useRef } from 'react'
import { Link, useSearchParams } from 'react-router-dom'

import { useNow } from '../../../hooks/useNow'
import { usePipelineViewQuery } from '../../../hooks/useUrlState'
import { useProviders } from '../../../queries/useProvidersQueries'
import { type ProviderTone, providerTone } from '../../../utils/fetchInfo'
import { usePipelineViews } from '../queries/usePipelineViewQueries'
import { activeTabId, providerTabId, tabSearch } from '../utils/providerTabs'
import classes from './PipelineViewStrip.module.css'
import { ToneDot } from './ToneDot'

const EDGE = 16

interface Chip {
  id: string
  label: string
  dot?: ProviderTone | undefined
}

export function PipelineViewStrip(): ReactNode {
  const query = usePipelineViewQuery()
  const [searchParams] = useSearchParams()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const now = useNow(30_000)
  const activeId = activeTabId(views, query)
  const scrollerRef = useRef<HTMLDivElement>(null)
  const providerChips: Chip[] = providers.map((provider) => ({
    id: providerTabId(provider.id),
    label: `${provider.name} ${provider.pipeline_count}`,
    dot: providerTone(provider, now),
  }))
  const chips = views
    .filter((view) => view.parent === null)
    .flatMap((view): Chip[] => (view.kind === 'pinned' ? providerChips : [{ id: view.id, label: view.name }]))
  const activeIndex = chips.findIndex((chip) => chip.id === activeId)

  useLayoutEffect(() => {
    const scroller = scrollerRef.current
    const chip = scroller?.children[activeIndex]

    if (!scroller || !(chip instanceof HTMLElement)) {
      return
    }

    const start = chip.offsetLeft - EDGE
    const end = chip.offsetLeft + chip.offsetWidth + EDGE - scroller.clientWidth

    if (start < scroller.scrollLeft) {
      scroller.scrollLeft = start
    } else if (end > scroller.scrollLeft) {
      scroller.scrollLeft = end
    }
  }, [activeIndex])

  return (
    <nav aria-label="Pipeline views" className={classes.strip}>
      <div ref={scrollerRef} className={classes.scroller}>
        {chips.map(({ id, label, dot }) => (
          <Link
            key={id}
            to={{ pathname: '/pipelines', search: tabSearch(searchParams, id).toString() }}
            replace
            className={classes.chip}
            aria-current={activeId === id ? 'page' : undefined}
          >
            {dot && <ToneDot tone={dot} />}
            <span className={classes.label}>{label}</span>
          </Link>
        ))}
      </div>
    </nav>
  )
}
