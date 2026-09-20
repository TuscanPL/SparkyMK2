#!/bin/sh
# Apply the SP-404MKII udev rule right away, including to a device that is already plugged in.
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  udevadm trigger --subsystem-match=tty --action=change >/dev/null 2>&1 || true
fi
exit 0
