import { Combobox, Menu, Modal, MultiSelect, Notification, Select } from '@mantine/core'

import classes from './iosOverlays.module.css'

const PRESENT_EASING = 'cubic-bezier(0.32, 0.72, 0, 1)'
const OVERLAY_Z = 499
const FLOATING_Z = 500
const FLOAT_BOTTOM = 'calc(max(8px, var(--safe-bottom)) + var(--keyboard-inset, 0px))'
const FLOAT_SIDE_LEFT = 'max(calc(8px + var(--safe-left)), calc((100vw - 480px) / 2))'
const FLOAT_SIDE_RIGHT = 'max(calc(8px + var(--safe-right)), calc((100vw - 480px) / 2))'

const overlayProps = { fixed: true, color: '#000', backgroundOpacity: 0.4, zIndex: OVERLAY_Z } as const
const slideUp = { transition: 'slide-up', duration: 280, timingFunction: PRESENT_EASING } as const

const floatingSheet = (bottom: string) =>
  ({
    position: 'fixed',
    top: 'auto',
    right: FLOAT_SIDE_RIGHT,
    bottom,
    left: FLOAT_SIDE_LEFT,
    width: 'auto',
  }) as const

const actionSheet = floatingSheet(FLOAT_BOTTOM)

const pickerProps = {
  classNames: { dropdown: classes.pickerDropdown, option: classes.pickerOption },
  styles: { dropdown: floatingSheet(FLOAT_BOTTOM) },
}

export { classes as iosOverlayClasses }

export const iosOverlayComponents = {
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
    defaultProps: { withCloseButton: true, closeButtonProps: { 'aria-label': 'Dismiss notification' } },
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
    defaultProps: { checkIconPosition: 'right', withScrollArea: false },
    ...pickerProps,
  }),
  MultiSelect: MultiSelect.extend({
    defaultProps: { checkIconPosition: 'right', withScrollArea: false },
    ...pickerProps,
  }),
}
