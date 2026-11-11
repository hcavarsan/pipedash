import type { ReactNode } from 'react'

type FilterOption = string | { value: string; label: string }

interface ValueFilter {
  value: string | null
  onChange: (value: string | null) => void
}

interface OptionFilter extends ValueFilter {
  options: FilterOption[]
  label?: string
  allLabel?: string
}

interface FilterConfig {
  search?: {
    value: string
    onChange: (value: string) => void
    placeholder?: string
  }
  status?: {
    value: string[]
    onSelect: (status: string, selected: boolean) => void
    onClear: () => void
    options?: { value: string; label: string }[]
  }
  provider?: OptionFilter
  organization?: OptionFilter
  repository?: OptionFilter
  workflow?: OptionFilter
  branch?: OptionFilter
  actor?: OptionFilter
  dateRange?: ValueFilter
}

export interface FilterSheet {
  opened: boolean
  onClose: () => void
}

export interface FilterBarProps {
  filters: FilterConfig
  onClearAll?: () => void
  sheet?: FilterSheet | undefined
}

export interface FilterSelectProps {
  label: string
  data: { value: string; label: string }[]
  value: string | null
  onChange: (value: string | null) => void
  searchable?: boolean | undefined
  leftSection?: ReactNode
  clearable?: boolean | undefined
}

export interface FilterSelectItem extends Pick<FilterSelectProps, 'label' | 'data' | 'value' | 'onChange'> {
  key: string
  searchable: boolean
}

export interface FilterSheetProps {
  opened: boolean
  onClose: () => void
  status: FilterConfig['status']
  statusOptions: { value: string; label: string }[]
  selects: FilterSelectItem[]
  hasActiveFilters: boolean
  onClear: () => void
}
