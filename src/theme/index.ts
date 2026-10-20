import { Badge, Card, createTheme, type MantineColorsTuple, Modal, Paper, Tooltip } from '@mantine/core'

const blue: MantineColorsTuple = [
  '#EBF3FF',
  '#D6E7FF',
  '#A8CEFF',
  '#7AB5FF',
  '#5B9EFF',
  '#4A8EEF',
  '#3A7FDF',
  '#2A6FCF',
  '#1A5FBF',
  '#0A4FAF',
]

const green: MantineColorsTuple = [
  '#E6FAF3',
  '#CCF5E7',
  '#99EBCF',
  '#66E1B7',
  '#3DD68C',
  '#32C17D',
  '#28AC6E',
  '#1E975F',
  '#148250',
  '#0A6D41',
]

const red: MantineColorsTuple = [
  '#FFE8E8',
  '#FFD1D1',
  '#FFA3A3',
  '#FF8B8B',
  '#FF6B6B',
  '#F05A5A',
  '#E04949',
  '#D03838',
  '#C02727',
  '#B01616',
]

const yellow: MantineColorsTuple = [
  '#FFFAE6',
  '#FFF5CC',
  '#FFEB99',
  '#FFE366',
  '#FFD93D',
  '#D9A514',
  '#A87A06',
  '#8C6504',
  '#715103',
  '#573E02',
]

const gray: MantineColorsTuple = [
  '#F9FAFB',
  '#F3F4F6',
  '#E5E7EB',
  '#D1D5DB',
  '#9CA3AF',
  '#6B7280',
  '#4B5563',
  '#374151',
  '#1F2937',
  '#111827',
]

export const theme = createTheme({
  primaryColor: 'blue',
  defaultRadius: 'sm',
  respectReducedMotion: true,
  colors: { blue, green, red, yellow, gray },
  components: {
    Badge: Badge.extend({ defaultProps: { tt: 'none' } }),
    Card: Card.extend({ defaultProps: { radius: 'md', withBorder: true } }),
    Paper: Paper.extend({ defaultProps: { radius: 'md' } }),
    Modal: Modal.extend({ defaultProps: { radius: 'md' } }),
    Tooltip: Tooltip.extend({ defaultProps: { withArrow: true } }),
  },
})
