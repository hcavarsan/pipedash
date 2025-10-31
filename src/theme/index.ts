import { createTheme, type MantineColorsTuple } from '@mantine/core'

// Brand color palettes
const modernBlue: MantineColorsTuple = [
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

const modernGreen: MantineColorsTuple = [
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

const modernRed: MantineColorsTuple = [
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

const modernYellow: MantineColorsTuple = [
  '#FFFAE6',
  '#FFF5CC',
  '#FFEB99',
  '#FFE366',
  '#FFD93D',
  '#F0CA2E',
  '#E0BB1F',
  '#D0AC10',
  '#C09D01',
  '#B08E00',
]

const modernTeal: MantineColorsTuple = [
  '#E6FAF8',
  '#CCF5F0',
  '#99EBE1',
  '#66E1D2',
  '#3DD6C3',
  '#32C1B0',
  '#28AC9D',
  '#1E978A',
  '#148277',
  '#0A6D64',
]

const modernGray: MantineColorsTuple = [
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

  colors: {
    blue: modernBlue,
    green: modernGreen,
    red: modernRed,
    yellow: modernYellow,
    teal: modernTeal,
    gray: modernGray,
  },
})
