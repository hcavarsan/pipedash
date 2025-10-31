import { Group, Title } from '@mantine/core'

interface TableHeaderProps {
  title: string;
  count?: number;
}

export const TableHeader = ({ title, count }: TableHeaderProps) => {
  return (
    <Group mb="sm" justify="space-between" align="center" h={52} style={{ minHeight: 52 }}>
      <Title order={3} size="h3" fw={600}>
        {title}
        {count !== undefined && ` (${count})`}
      </Title>
    </Group>
  )
}
