import { Box, Group, Text, UnstyledButton } from '@mantine/core'
import { Link, useSearchParams } from 'react-router-dom'

import { useNow } from '../../hooks/useNow'
import { usePipelineViewQuery } from '../../hooks/useUrlState'
import { usePipelineViews } from '../../queries/usePipelineViewQueries'
import { useProviders } from '../../queries/useProvidersQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import { type ProviderTone, providerTone } from '../../utils/fetchInfo'
import { activeTabId, providerTabId, tabSearch } from '../../utils/providerTabs'
import { ToneDot } from '../atoms/ToneDot'

export function PipelineViewStrip() {
  const query = usePipelineViewQuery()
  const [searchParams] = useSearchParams()
  const { data: views = [] } = usePipelineViews()
  const { data: providers = [] } = useProviders()
  const now = useNow(30_000)
  const openManage = usePipelineViewStore((s) => s.openManage)
  const activeId = activeTabId(views, query)

  const chip = (id: string, label: string, dot?: ProviderTone) => {
    const active = activeId === id

    return (
      <UnstyledButton
        key={id}
        component={Link}
        to={{ pathname: '/pipelines', search: tabSearch(searchParams, id).toString() }}
        replace
        className="interactive-surface"
        aria-current={active ? 'page' : undefined}
        bdrs="sm"
        py="2xs"
        px="sm"
        flex="0 0 auto"
        c={active ? 'blue' : 'dimmed'}
        mod={{ current: active }}
      >
        <Group gap="3xs" wrap="nowrap">
          {dot && <ToneDot tone={dot} />}
          <Text size="xs" fw={500} truncate>
            {label}
          </Text>
        </Group>
      </UnstyledButton>
    )
  }

  const providerChips = providers.map((provider) =>
    chip(providerTabId(provider.id), `${provider.name} ${provider.pipeline_count}`, providerTone(provider, now))
  )

  return (
    <Box
      component="nav"
      aria-label="Pipeline views"
      p="3xs"
      style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}
    >
      <Box style={{ display: 'flex', gap: 'var(--mantine-spacing-3xs)', overflowX: 'auto' }}>
        {views
          .filter((view) => view.parent === null)
          .flatMap((view) => (view.kind === 'pinned' ? providerChips : [chip(view.id, view.name)]))}
        <UnstyledButton
          className="interactive-surface"
          bdrs="sm"
          py="2xs"
          px="sm"
          flex="0 0 auto"
          c="dimmed"
          ml="auto"
          onClick={openManage}
        >
          <Text size="xs" fw={500}>
            Edit
          </Text>
        </UnstyledButton>
      </Box>
    </Box>
  )
}
