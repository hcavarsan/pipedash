import {
  ActionIcon,
  Button,
  Checkbox,
  Combobox,
  createTheme,
  Input,
  InputWrapper,
  Menu,
  Modal,
  MultiSelect,
  mergeThemeOverrides,
  Notification,
  SegmentedControl,
  Select,
  Stepper,
  Switch,
  Tabs,
} from '@mantine/core'

import { theme } from './index'
import classes from './ios.module.css'

const SYSTEM_FONT = '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", system-ui, sans-serif'
const SPRING = 'cubic-bezier(0.32, 0.72, 0, 1)'
const OVERLAY_Z = 499
const FLOATING_Z = 500

const overlayProps = { fixed: true, color: '#000', backgroundOpacity: 0.4, zIndex: OVERLAY_Z } as const
const slideUp = { transition: 'slide-up', duration: 280, timingFunction: SPRING } as const

const bottomSheet = {
  position: 'fixed',
  top: 'auto',
  right: 0,
  bottom: 0,
  left: 0,
  width: '100%',
} as const

const actionSheet = {
  position: 'fixed',
  top: 'auto',
  right: 8,
  bottom: 'calc(var(--safe-bottom) + 8px)',
  left: 8,
  width: 'auto',
} as const

const pickerProps = {
  classNames: { dropdown: classes.pickerDropdown, option: classes.pickerOption },
  styles: { dropdown: bottomSheet },
}

export const iosToastClassNames = { root: classes.toastRoot }
export { classes as iosClasses }

const iosOverrides = createTheme({
  fontFamily: SYSTEM_FONT,
  headings: {
    fontFamily: SYSTEM_FONT,
    fontWeight: '700',
    sizes: {
      h1: { fontSize: '34px', lineHeight: '1.15' },
      h2: { fontSize: '28px', lineHeight: '1.2' },
      h3: { fontSize: '22px', lineHeight: '1.25' },
      h4: { fontSize: '20px', lineHeight: '1.3' },
      h5: { fontSize: '17px', lineHeight: '1.35' },
      h6: { fontSize: '15px', lineHeight: '1.35' },
    },
  },
  fontSizes: { xs: '13px', sm: '15px', md: '17px', lg: '20px', xl: '22px' },
  radius: { xs: '6px', sm: '10px', md: '12px', lg: '16px', xl: '24px' },
  defaultRadius: 'sm',
  components: {
    Modal: Modal.extend({
      defaultProps: {
        radius: 0,
        transitionProps: slideUp,
        overlayProps: { color: '#000', backgroundOpacity: 0.45 },
        closeButtonProps: { children: 'Done', 'aria-label': 'Close' },
      },
      classNames: {
        inner: classes.sheetInner,
        content: classes.sheetContent,
        header: classes.sheetHeader,
        title: classes.sheetTitle,
        close: classes.sheetClose,
        body: classes.sheetBody,
      },
    }),
    Notification: Notification.extend({
      classNames: {
        root: classes.toast,
        title: classes.toastTitle,
        description: classes.toastDescription,
        closeButton: classes.toastClose,
      },
    }),
    Menu: Menu.extend({
      defaultProps: {
        withOverlay: true,
        overlayProps,
        transitionProps: slideUp,
        zIndex: FLOATING_Z,
        withinPortal: true,
      },
      classNames: {
        dropdown: classes.menuDropdown,
        item: classes.menuItem,
        itemLabel: classes.menuItemLabel,
        itemSection: classes.menuItemSection,
        label: classes.menuLabel,
        divider: classes.menuDivider,
      },
      styles: { dropdown: actionSheet },
    }),
    Combobox: Combobox.extend({
      defaultProps: {
        width: '100%',
        withOverlay: true,
        overlayProps,
        transitionProps: slideUp,
        zIndex: FLOATING_Z,
        withinPortal: true,
      },
    }),
    Select: Select.extend({
      defaultProps: { checkIconPosition: 'right', maxDropdownHeight: 320 },
      ...pickerProps,
    }),
    MultiSelect: MultiSelect.extend({
      defaultProps: { checkIconPosition: 'right', maxDropdownHeight: 320 },
      ...pickerProps,
    }),
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
