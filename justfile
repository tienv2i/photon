# Photon Developer & Agent Task Entrypoint

_default:
    @just --list

build:
    cargo build --workspace

build-release:
    cargo build --workspace --release

test *args:
    cargo test --workspace {{args}}

test-core:
    cargo test -p photon-protocol -p photon-storage -p photon-i18n -p photon-image -p photon-core -p photon-ipc -p photon-cli

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets

run *args:
    cargo run -p photon-desktop --bin photon -- {{args}}

run-headless *args:
    cargo run -p photon-desktop --bin photon -- --headless {{args}}

ctl *args:
    cargo run -p photon-cli --bin photonctl -- {{args}}

logs *args:
    cargo run -p photon-cli --bin photonctl -- debug logs {{args}}

journal *args:
    cargo run -p photon-cli --bin photonctl -- debug journal {{args}}

replay *args:
    cargo run -p photon-cli --bin photonctl -- debug replay {{args}}

schema:
    cargo run -p photon-cli --bin photonctl -- schema

# Build release and install system-wide into PREFIX (default: /usr/local)
install *args:
    python3 scripts/install.py install {{args}}

# Install into ~/.local (no sudo) with Viewer & Editor MIME types
install-user *args:
    python3 scripts/install.py install --user {{args}}

# Install reusing the debug build (fast dev iteration, shares prod identity)
install-dev *args:
    python3 scripts/install.py install --dev {{args}}

# Uninstall from PREFIX (default: /usr/local)
uninstall *args:
    python3 scripts/install.py uninstall {{args}}

# Uninstall from ~/.local
uninstall-user *args:
    python3 scripts/install.py uninstall --user {{args}}

# Build all 9 standalone plugin crates in release mode
build-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/photon-plugin-*; do
        echo "Building $dir..."
        cargo build --manifest-path "$dir/Cargo.toml" --release
    done

# Build and install all 9 standalone plugins into the Photon plugins directory (profile: prod | dev)
install-plugins profile="prod": build-plugins
    #!/usr/bin/env bash
    set -euo pipefail
    data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    if [ -n "${PHOTON_PLUGINS_DIR:-}" ]; then
        plugins_root="$PHOTON_PLUGINS_DIR"
    elif [ "{{profile}}" = "dev" ]; then
        plugins_root="$data_home/photon-dev/plugins"
    else
        plugins_root="$data_home/photon/plugins"
    fi
    mkdir -p "$plugins_root"
    for dir in plugins/photon-plugin-*; do
        manifest="$dir/manifest.json"
        plugin_id="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["id"])' "$manifest")"
        entry_cmd="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["entry"]["command"])' "$manifest")"
        bin_name="$(basename "$entry_cmd")"
        built_bin="$dir/target/release/$bin_name"
        if [ ! -f "$built_bin" ]; then
            echo "error: built plugin binary missing: $built_bin" >&2
            exit 1
        fi
        dest="$plugins_root/$plugin_id"
        rm -rf "$dest"
        mkdir -p "$dest/$(dirname "$entry_cmd")"
        cp "$manifest" "$dest/manifest.json"
        install -m 755 "$built_bin" "$dest/$entry_cmd"
        if [ -d "$dir/assets" ]; then
            cp -r "$dir/assets" "$dest/assets"
        fi
        echo "Installed $plugin_id -> $dest"
    done

