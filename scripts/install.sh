#!/usr/bin/env bash
# Install Aura Harness for the current user: binary, launcher, icon, entry.
#
#   ./scripts/install.sh            # build release + install
#   ./scripts/install.sh --no-build # install whatever is already built
#
# The desktop entry always goes through aura-harness-launch, which sets up
# the Wayland context before exec'ing the binary (see CLAUDE.md machine
# rules). Never point the entry straight at the binary.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
app_dir="${HOME}/.local/share/applications"
icon_dir="${HOME}/.local/share/icons/hicolor/64x64/apps"

if [[ "${1:-}" != "--no-build" ]]; then
    echo "building release…"
    cargo build --release --manifest-path "${repo}/Cargo.toml" -p harness-app
fi

binary="${repo}/target/release/aura-harness"
if [[ ! -x "${binary}" ]]; then
    echo "error: ${binary} not found — run without --no-build first" >&2
    exit 1
fi

mkdir -p "${bin_dir}" "${app_dir}" "${icon_dir}"
install -m 0755 "${binary}" "${bin_dir}/aura-harness"

cat > "${bin_dir}/aura-harness-launch" <<LAUNCHER
#!/usr/bin/env bash
# Aura Harness launcher — ensures correct Wayland/X context.
if [ -z "\$WAYLAND_DISPLAY" ]; then
    export WAYLAND_DISPLAY=wayland-1
fi
if [ -z "\$XDG_RUNTIME_DIR" ]; then
    export XDG_RUNTIME_DIR=/run/user/\$(id -u)
fi
exec "${bin_dir}/aura-harness" "\$@"
LAUNCHER
chmod 0755 "${bin_dir}/aura-harness-launch"

install -m 0644 "${repo}/assets/icon.png" "${icon_dir}/aura-harness.png"
install -m 0644 "${repo}/assets/aura-harness.desktop" "${app_dir}/aura-harness.desktop"

# Refresh caches; absence of either tool is fine.
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${app_dir}" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "installed:"
echo "  ${bin_dir}/aura-harness"
echo "  ${bin_dir}/aura-harness-launch"
echo "  ${app_dir}/aura-harness.desktop"
echo "  ${icon_dir}/aura-harness.png"

case ":${PATH}:" in
    *":${bin_dir}:"*) ;;
    *)
        echo
        echo "note: ${bin_dir} is not on PATH — add it, or run ${bin_dir}/aura-harness-launch"
        ;;
esac
