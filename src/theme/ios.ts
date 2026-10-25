import {
  ActionIcon,
  Button,
  Checkbox,
  type CSSVariablesResolver,
  createTheme,
  Input,
  InputWrapper,
  type MantineColorsTuple,
  mergeThemeOverrides,
  SegmentedControl,
  Stepper,
  Switch,
  Tabs,
  v8CssVariablesResolver,
} from '@mantine/core'

import { theme } from './index'
import classes from './ios.module.css'
import { iosOverlayClasses, iosOverlayComponents } from './iosOverlays'

const SYSTEM_FONT = '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", system-ui, sans-serif'

export const iosToastClassNames = { root: iosOverlayClasses.toastRoot }
export const iosClasses = { ...classes, ...iosOverlayClasses }

const blue: MantineColorsTuple = [
  '#E6F1FF',
  '#CCE3FF',
  '#99C8FF',
  '#66ACFF',
  '#3391FF',
  '#0A84FF',
  '#007AFF',
  '#0062CC',
  '#004A99',
  '#0040DD',
]

const red: MantineColorsTuple = [
  '#FFEBEA',
  '#FFD6D3',
  '#FFAEA8',
  '#FF857D',
  '#FF5E55',
  '#FF453A',
  '#FF3B30',
  '#D70015',
  '#B3000F',
  '#8C000C',
]

const green: MantineColorsTuple = [
  '#E8F9ED',
  '#CFF3DA',
  '#9FE7B5',
  '#6FDB90',
  '#4AD16F',
  '#30D158',
  '#34C759',
  '#248A3D',
  '#1D7131',
  '#165826',
]

const tinted = (name: string, token: string) => ({
  [`--mantine-color-${name}-text`]: `var(${token})`,
  [`--mantine-color-${name}-filled`]: `var(${token})`,
  [`--mantine-color-${name}-filled-hover`]: `color-mix(in srgb, var(${token}) 85%, var(--ios-label))`,
  [`--mantine-color-${name}-light`]: `color-mix(in srgb, var(${token}) 15%, transparent)`,
  [`--mantine-color-${name}-light-hover`]: `color-mix(in srgb, var(${token}) 22%, transparent)`,
  [`--mantine-color-${name}-light-color`]: `var(${token})`,
  [`--mantine-color-${name}-outline`]: `var(${token})`,
  [`--mantine-color-${name}-outline-hover`]: `color-mix(in srgb, var(${token}) 5%, transparent)`,
})

const IOS_ROLES = {
  '--mantine-color-body': 'var(--ios-grouped-bg)',
  '--mantine-color-text': 'var(--ios-label)',
  '--mantine-color-bright': 'var(--ios-label)',
  '--mantine-color-dimmed': 'var(--ios-secondary-label)',
  '--mantine-color-placeholder': 'var(--ios-tertiary-label)',
  '--mantine-color-default': 'var(--ios-cell)',
  '--mantine-color-default-color': 'var(--ios-label)',
  '--mantine-color-default-hover': 'var(--ios-fill-tertiary)',
  '--mantine-color-default-border': 'var(--ios-separator)',
  '--mantine-color-anchor': 'var(--ios-tint)',
  '--mantine-color-error': 'var(--ios-red)',
  '--mantine-color-success': 'var(--ios-green)',
  ...tinted('blue', '--ios-tint'),
  ...tinted('red', '--ios-red'),
  ...tinted('green', '--ios-green'),
}

export const iosCssVariablesResolver: CSSVariablesResolver = (mantineTheme) => {
  const base = v8CssVariablesResolver(mantineTheme)

  return {
    variables: base.variables,
    light: { ...base.light, ...IOS_ROLES },
    dark: { ...base.dark, ...IOS_ROLES },
  }
}

const iosOverrides = createTheme({
  fontFamily: SYSTEM_FONT,
  colors: { blue, red, green },
  breakpoints: { xs: '36em', sm: '60em', md: '62em', lg: '75em', xl: '88em' },
  headings: {
    fontFamily: SYSTEM_FONT,
    fontWeight: '400',
    sizes: {
      h1: { fontSize: 'var(--ios-fs-large-title)', lineHeight: 'var(--ios-lh-large-title)', fontWeight: '700' },
      h2: { fontSize: 'var(--ios-fs-title1)', lineHeight: 'var(--ios-lh-title1)' },
      h3: { fontSize: 'var(--ios-fs-title2)', lineHeight: 'var(--ios-lh-title2)' },
      h4: { fontSize: 'var(--ios-fs-title3)', lineHeight: 'var(--ios-lh-title3)' },
      h5: { fontSize: 'var(--ios-fs-headline)', lineHeight: 'var(--ios-lh-headline)', fontWeight: '600' },
      h6: { fontSize: 'var(--ios-fs-subheadline)', lineHeight: 'var(--ios-lh-subheadline)', fontWeight: '600' },
    },
  },
  fontSizes: {
    xs: 'var(--ios-fs-footnote)',
    sm: 'var(--ios-fs-subheadline)',
    md: 'var(--ios-fs-body)',
    lg: 'var(--ios-fs-title3)',
    xl: 'var(--ios-fs-title2)',
  },
  lineHeights: {
    xs: 'var(--ios-lh-footnote)',
    sm: 'var(--ios-lh-subheadline)',
    md: 'var(--ios-lh-body)',
    lg: 'var(--ios-lh-title3)',
    xl: 'var(--ios-lh-title2)',
  },
  radius: { xs: '6px', sm: '10px', md: '12px', lg: '16px', xl: '24px' },
  defaultRadius: 'sm',
  components: {
    ...iosOverlayComponents,
    Tabs: Tabs.extend({
      classNames: { list: classes.tabsList, tab: classes.tab, panel: classes.tabPanel },
    }),
    SegmentedControl: SegmentedControl.extend({
      classNames: { root: classes.segmentRoot, indicator: classes.segmentIndicator, label: classes.segmentLabel },
    }),
    Switch: Switch.extend({
      defaultProps: { withThumbIndicator: false },
      classNames: { track: classes.switchTrack, thumb: classes.switchThumb, label: classes.switchLabel },
    }),
    Input: Input.extend({ classNames: { input: classes.input } }),
    InputWrapper: InputWrapper.extend({
      classNames: { label: classes.inputLabel, description: classes.inputDescription, error: classes.inputError },
    }),
    Button: Button.extend({ classNames: { root: classes.button } }),
    ActionIcon: ActionIcon.extend({ classNames: { root: classes.actionIcon } }),
    Checkbox: Checkbox.extend({ classNames: { input: classes.checkboxInput, label: classes.checkboxLabel } }),
    Stepper: Stepper.extend({
      classNames: {
        steps: classes.stepperSteps,
        step: classes.stepperStep,
        stepIcon: classes.stepperStepIcon,
        stepBody: classes.stepperStepBody,
        separator: classes.stepperSeparator,
        content: classes.stepperContent,
      },
    }),
  },
})

export const iosTheme = mergeThemeOverrides(theme, iosOverrides)
