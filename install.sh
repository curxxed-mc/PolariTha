#!/bin/sh
# Build and install PolariTha for the current user, plus the udev rule (needs sudo).
set -e
cd "$(dirname "$0")"
cargo build --release
install -Dm755 target/release/polaritha "$HOME/.local/bin/polaritha"
install -Dm644 packaging/polaritha.desktop "$HOME/.local/share/applications/polaritha.desktop"
install -Dm644 assets/icons/polaritha.svg "$HOME/.local/share/icons/hicolor/scalable/apps/polaritha.svg"
for size in 32 48 64 128 256 512; do
    install -Dm644 "assets/icons/polaritha-$size.png" "$HOME/.local/share/icons/hicolor/${size}x${size}/apps/polaritha.png"
done
echo "Installing udev rule (sudo)…"
sudo install -Dm644 packaging/70-polaritha.rules /etc/udev/rules.d/70-polaritha.rules
echo "Installing libinput quirk (sudo)…"
sudo install -Dm644 packaging/polaritha.quirks /etc/libinput/local-overrides.quirks
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
echo "Done. Run 'polaritha' or start PolariTha from the application menu."
