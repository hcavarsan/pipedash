import {
  Badge,
  Card,
  Combobox,
  createTheme,
  Drawer,
  type MantineColorsTuple,
  Menu,
  Modal,
  mergeThemeOverrides,
  Notification,
  Paper,
  PasswordInput,
  Popover,
  rem,
  Title,
  Tooltip,
} from '@mantine/core'

import { MODAL_TRANSITION, OVERLAY_TRANSITION, PRESENT_EASING } from '../lib/motion'

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

export const SPACING_PX = { '4xs': 2, '3xs': 4, '2xs': 6, xs: 10, sm: 12, md: 16, lg: 20, xl: 32 } as const

export const baseTheme = createTheme({
  primaryColor: 'blue',
  defaultRadius: 'sm',
  respectReducedMotion: true,
  colors: { blue, green, red, yellow, gray },
  spacing: Object.fromEntries(Object.entries(SPACING_PX).map(([name, value]) => [name, rem(value)])),
  components: {
    Badge: Badge.extend({ defaultProps: { tt: 'none' } }),
    Card: Card.extend({ defaultProps: { radius: 'md', withBorder: true } }),
    Paper: Paper.extend({ defaultProps: { radius: 'md' } }),
    Modal: Modal.extend({ defaultProps: { radius: 'md' } }),
    Tooltip: Tooltip.extend({ defaultProps: { withArrow: true } }),
  },
})

export const theme = mergeThemeOverrides(
  baseTheme,
  createTheme({
    autoContrast: true,
    luminanceThreshold: 0.183,
    primaryShade: { light: 7, dark: 7 },
    lineHeights: { xs: '1.5', sm: '1.5', md: '1.5', lg: '1.5', xl: '1.5' },
    headings: {
      sizes: {
        h1: { lineHeight: '1.15' },
        h2: { lineHeight: '1.2' },
        h3: { lineHeight: '1.25' },
        h4: { lineHeight: '1.3' },
        h5: { lineHeight: '1.35' },
        h6: { lineHeight: '1.4' },
      },
    },
    components: {
      Title: Title.extend({ classNames: { root: 'desktop-heading' } }),
      Modal: Modal.extend({
        defaultProps: {
          radius: 'md',
          transitionProps: MODAL_TRANSITION,
          closeButtonProps: { 'aria-label': 'Close dialog' },
        },
      }),
      Drawer: Drawer.extend({
        defaultProps: {
          transitionProps: { duration: 160, timingFunction: PRESENT_EASING },
          closeButtonProps: { 'aria-label': 'Close dialog' },
        },
      }),
      Menu: Menu.extend({ defaultProps: { shadow: 'md', transitionProps: OVERLAY_TRANSITION } }),
      Popover: Popover.extend({ defaultProps: { shadow: 'md', transitionProps: OVERLAY_TRANSITION } }),
      Combobox: Combobox.extend({ defaultProps: { shadow: 'md', transitionProps: OVERLAY_TRANSITION } }),
      Tooltip: Tooltip.extend({
        defaultProps: {
          withArrow: true,
          events: { hover: true, focus: true, touch: false },
          transitionProps: { transition: 'fade', duration: 100, timingFunction: PRESENT_EASING },
        },
      }),
      Notification: Notification.extend({ defaultProps: { closeButtonProps: { 'aria-label': 'Close notification' } } }),
      PasswordInput: PasswordInput.extend({ defaultProps: { visibilityToggleFocusable: true } }),
    },
  })
)
