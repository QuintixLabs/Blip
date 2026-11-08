#!/bin/sh
# After installing: apply Blip's mouse rule (70-blip.rules) to the mice already
# plugged in, so blips work on Wayland right away.
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger --subsystem-match=input --action=change 2>/dev/null || true
