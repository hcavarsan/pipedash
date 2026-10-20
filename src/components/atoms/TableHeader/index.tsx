import { Group, Title } from '@mantine/core'

import { useIsMobile } from '../../../hooks/useIsMobile'

interface TableHeaderProps {
  title: string
  count?: number
}

export const TableHeader = ({ title, count }: TableHeaderProps) => {
  const { isMobile } = useIsMobile()

  return (
    <Group mb="xs" justify="space-between" align="center" h={isMobile ? 32 : 40}>
      <Title order={2} fw={600} size={isMobile ? 'h5' : 'h4'}>
        {title}
        {count !== undefined && ` (${count})`}
      </Title>
    </Group>
  )
}
