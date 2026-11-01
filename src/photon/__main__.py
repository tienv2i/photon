"""Entry point for photon CLI."""

from __future__ import annotations

import sys


def main() -> None:
    """Main entry point."""
    from photon.app import run

    sys.exit(run(sys.argv[1:]))


if __name__ == "__main__":
    main()
