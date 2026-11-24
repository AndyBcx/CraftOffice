#!/usr/bin/env bash
# CraftOffice web: copy the site to /opt/craftoffice-web. Run from the unpacked
# craftoffice-web folder, as root:   sudo ./install.sh
# It touches nothing else (no other service, no other site); HTTPS: see Caddyfile.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "run as root (sudo)"; exit 1; }
cd "$(dirname "$0")"
[ -f site/index.html ] || { echo "site/ is missing: run this from the unpacked craftoffice-web folder"; exit 1; }
install -d /opt/craftoffice-web
rm -rf /opt/craftoffice-web.new
cp -r site /opt/craftoffice-web.new
rm -rf /opt/craftoffice-web.old
mv /opt/craftoffice-web /opt/craftoffice-web.old
mv /opt/craftoffice-web.new /opt/craftoffice-web
rm -rf /opt/craftoffice-web.old
chmod -R a+rX /opt/craftoffice-web
install -d /etc/caddy
install -m 0644 Caddyfile /etc/caddy/craftoffice.Caddyfile.example
echo "CraftOffice web is in /opt/craftoffice-web."
echo "Quick test on this machine:  python3 -m http.server 8090 -d /opt/craftoffice-web  → http://127.0.0.1:8090/"
echo "For the phone: add a block from /etc/caddy/craftoffice.Caddyfile.example to /etc/caddy/Caddyfile, then"
echo "  sudo systemctl reload caddy   and open the HTTPS address in Chrome → menu → Install app."
