import { Anchor, Box, Breadcrumbs, Group, Text, UnstyledButton } from '@mantine/core'
import { IconChevronRight, IconPlus } from '@tabler/icons-react'
import { Link, useSearchParams } from 'react-router-dom'

import { usePipelineViewQuery } from '../../hooks/useUrlState'
import { usePipelineViews } from '../../queries/usePipelineViewQueries'
import { usePipelineViewStore } from '../../stores/pipelineViewStore'
import { canAddSubView, childViews, viewAncestors } from '../../utils/viewTree'

export function SubViewBar() {
  const query = usePipelineViewQuery()
  const [searchParams] = useSearchParams()
  const { data: views = [] } = usePipelineViews()
  const openEditor = usePipelineViewStore((s) => s.openEditor)
  const current = views.find((view) => view.id === query.view)

  if (!current) {
    return null
  }

  const ancestors = viewAncestors(views, current.id)
  const children = childViews(views, current.id)

  if (ancestors.length === 0 && children.length === 0) {
    return null
  }

  const to = (id: string) => {
    const next = new URLSearchParams(searchParams)

    next.set('view', id)

    return { pathname: '/pipelines', search: next.toString() }
  }

  return (
    <Box
      component="nav"
      aria-label="Sub-views"
      style={{ display: 'flex', alignItems: 'center', gap: 'var(--mantine-spacing-sm)', overflowX: 'auto' }}
    >
      {ancestors.length > 0 && (
        <Breadcrumbs
          separator={<IconChevronRight size={12} aria-hidden />}
          separatorMargin="3xs"
          style={{ flexWrap: 'nowrap', flex: '0 0 auto' }}
        >
          {ancestors.map((view) => (
            <Anchor key={view.id} component={Link} to={to(view.id)} replace size="xs" c="dimmed" td="none" lh={1.4}>
              {view.name}
            </Anchor>
          ))}
          <Text size="xs" fw={600} aria-current="page">
            {current.name}
          </Text>
        </Breadcrumbs>
      )}
      <Box style={{ display: 'flex', gap: 'var(--mantine-spacing-3xs)', flex: '0 0 auto' }}>
        {children.map((view) => (
          <UnstyledButton
            key={view.id}
            component={Link}
            to={to(view.id)}
            replace
            className="interactive-surface"
            bdrs="sm"
            py="2xs"
            px="sm"
            bd="1px solid var(--mantine-color-default-border)"
          >
            <Group gap="3xs" wrap="nowrap">
              <Text size="xs" fw={500} span>
                {view.name}
              </Text>
              <Text size="xs" c="dimmed" span>
                {view.count}
              </Text>
            </Group>
          </UnstyledButton>
        ))}
        {canAddSubView(views, current) && (
          <UnstyledButton
            className="interactive-surface"
            bdrs="sm"
            py="2xs"
            px="sm"
            c="dimmed"
            onClick={() => openEditor(null, [], current.id)}
          >
            <Group gap="3xs" wrap="nowrap">
              <IconPlus size={12} aria-hidden />
              <Text size="xs" fw={500} span>
                New sub-view
              </Text>
            </Group>
          </UnstyledButton>
        )}
      </Box>
    </Box>
  )
}
