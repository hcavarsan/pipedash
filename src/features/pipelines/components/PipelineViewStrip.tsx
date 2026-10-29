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

export function PipelineViewStrip(): ReactNode {
  const query = usePipelineViewQuery()
  const [searchParams] = useSearchParams()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const now = useNow(30_000)
  const activeId = activeTabId(views, query)
  const scrollerRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const scroller = scrollerRef.current
    const chip = scroller?.querySelector<HTMLElement>(`[data-chip="${CSS.escape(activeId)}"]`)

    if (!scroller || !chip) {
      return
    }

    const start = chip.offsetLeft - EDGE
    const end = chip.offsetLeft + chip.offsetWidth + EDGE - scroller.clientWidth

    if (start < scroller.scrollLeft) {
      scroller.scrollLeft = start
    } else if (end > scroller.scrollLeft) {
      scroller.scrollLeft = end
    }
  }, [activeId])

  const chip = (id: string, label: string, dot?: ProviderTone) => {
    const active = activeId === id

    return (
      <Link
        key={id}
        to={{ pathname: '/pipelines', search: tabSearch(searchParams, id).toString() }}
        replace
        className={classes.chip}
        aria-current={active ? 'page' : undefined}
        data-chip={id}
      >
        {dot && <ToneDot tone={dot} />}
        <span className={classes.label}>{label}</span>
      </Link>
    )
  }

  const providerChips = providers.map((provider) =>
    chip(providerTabId(provider.id), `${provider.name} ${provider.pipeline_count}`, providerTone(provider, now))
  )

  return (
    <nav aria-label="Pipeline views" className={classes.strip}>
      <div ref={scrollerRef} className={classes.scroller}>
        {views
          .filter((view) => view.parent === null)
          .flatMap((view) => (view.kind === 'pinned' ? providerChips : [chip(view.id, view.name)]))}
      </div>
    </nav>
  )
}
