export interface SwipeAction {
  key: string
  label: string
  tone: 'blue' | 'red' | 'orange' | 'gray' | 'green'
  onSelect: () => void
  primary?: boolean
}

export const ACTION_WIDTH = 80
