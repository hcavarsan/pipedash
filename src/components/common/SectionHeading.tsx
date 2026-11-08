import { Group, Text, UnstyledButton } from '@mantine/core'
import { IconChevronDown, IconChevronRight } from '@tabler/icons-react'
import type { ReactNode } from 'react'

type SectionExpansion = { onToggle: () => void; expanded: boolean } | { onToggle?: never; expanded?: never }

type SectionHeadingProps = {
  id?: string | undefined
  icon: ReactNode
  label: string
  info?: ReactNode
  actions?: ReactNode
  count: number
} & SectionExpansion

export function SectionHeading({
  id,
  icon,
  label,
  info,
  actions,
  count,
  expanded,
  onToggle,
}: SectionHeadingProps): ReactNode {
  const content = (
    <Group gap="xs" wrap="nowrap">
      {onToggle &&
        (expanded ? (
          <IconChevronDown size={16} style={{ flexShrink: 0 }} aria-hidden />
        ) : (
          <IconChevronRight size={16} style={{ flexShrink: 0 }} aria-hidden />
        ))}
      {icon}
      <Text id={id} size="sm" fw={600} truncate>
        {label}
      </Text>
      <Text size="sm" c="dimmed">
        {count}
      </Text>
    </Group>
  )

  const heading = onToggle ? (
    <UnstyledButton
      className="item-primary"
      px="2xs"
      py="3xs"
      miw={0}
      maw="100%"
      onClick={onToggle}
      aria-expanded={expanded}
    >
      {content}
    </UnstyledButton>
  ) : (
    content
  )

  return info || actions || onToggle ? (
    <Group
      gap="3xs"
      wrap="nowrap"
      maw="100%"
      bdrs="sm"
      className={onToggle ? 'interactive-surface stretched' : undefined}
      style={{ alignSelf: 'flex-start' }}
    >
      {heading}
      {info}
      {actions}
    </Group>
  ) : (
    heading
  )
}
