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
