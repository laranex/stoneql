#!/bin/sh
# Installs a .deb with apt in a fresh container and launches it under Xvfb on a session bus.
# Run as root in ubuntu:22.04 or debian:12 with the package mounted read-only and /out writable:
#   smoke.sh /deb/StoneQL_<version>_<sha>_amd64.deb
# Writes /out/install.txt, /out/app.log, /out/window.txt and /out/screen.png; exits non-zero if
# the app is not alive after WAIT seconds or no window titled StoneQL exists.
set -eu
if [ "${1:-}" = "--launch" ]; then
  bin="$2"
  "$bin" > /out/app.log 2>&1 &
  pid=$!
  sleep "${WAIT:-30}"
  alive=no; kill -0 "$pid" 2>/dev/null && alive=yes
  {
    echo "alive_after_${WAIT:-30}s=$alive"
    echo "xdotool: $(xdotool search --name '^StoneQL$' 2>/dev/null | tr '\n' ' ')"
    xwininfo -root -tree | grep '"StoneQL"' || echo "xwininfo: no StoneQL window"
  } > /out/window.txt
  import -window root /out/screen.png 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  [ "$alive" = yes ] && grep -q '"StoneQL"' /out/window.txt
  exit
fi
deb="$1"
pkg="$(dpkg-deb -f "$deb" Package)"
export DEBIAN_FRONTEND=noninteractive
{
  grep PRETTY_NAME /etc/os-release
  sha256sum "$deb"
  echo "Depends: $(dpkg-deb -f "$deb" Depends)"
} > /out/install.txt
apt-get update -qq
apt-get install -y -qq --no-install-recommends "$deb" >> /out/install.txt 2>&1
apt-get install -y -qq --no-install-recommends xvfb xauth x11-utils xdotool dbus imagemagick > /dev/null 2>&1
dpkg-query -W -f='${Package} ${Version}\n' libc6 libwebkit2gtk-4.1-0 libgtk-3-0 libdbus-1-3 "$pkg" >> /out/install.txt
bin="$(dpkg -L "$pkg" | grep '^/usr/bin/' | head -n1)"
echo "binary: $bin" >> /out/install.txt
useradd -m tester
chmod 0777 /out
su tester -c "WAIT=${WAIT:-30} dbus-run-session -- xvfb-run -a -s '-screen 0 1440x960x24' sh $0 --launch $bin"
