# loginw

"generalized `weston-launch`" / "`logind` without the `d`"

A setuid launcher / *w*rapper that passes GPU/vt/input file descriptors to an unprivileged display manager (typically, a Wayland compositor) and controls the virtual terminal / DRM master.
Also provides shutdown/reboot/suspend commands like logind does.
But does not support any multiseat stuff.

Currently supports FreeBSD only, but can be ported to Linux.
