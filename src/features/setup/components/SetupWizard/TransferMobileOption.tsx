import { IconCheck, type TablerIcon } from '@tabler/icons-react'
import type { ReactNode } from 'react'
import { IosRow } from '../../../../components/ios/IosList'

interface TransferMobileOptionProps {
  Icon: TablerIcon
  title: string
  subtitle: string
  selected: boolean
  onSelect: () => void
}

export function TransferMobileOption({
  Icon,
  title,
  subtitle,
  selected,
  onSelect,
}: TransferMobileOptionProps): ReactNode {
  return (
    <IosRow
      icon={<Icon size={18} aria-hidden />}
      title={title}
      subtitle={subtitle}
      selected={selected}
      accessory={
        selected ? <IconCheck size={20} color="var(--mantine-color-blue-light-color)" aria-hidden /> : undefined
      }
      onClick={onSelect}
    />
  )
}
