"""Standardized error codes & PhotonError classes."""

from __future__ import annotations


class PhotonError(Exception):
    """Base exception for Photon errors.

    Attributes:
        code: Error code (e.g., "PHOTON-DEC-101")
        message: Human-readable message
    """

    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(f"{code}: {message}")


# File errors
class FileNotFoundError_(PhotonError):
    """File not found."""

    def __init__(self, path: str):
        super().__init__("PHOTON-FILE-101", f"File not found: {path}")


class PermissionDeniedError(PhotonError):
    """Permission denied."""

    def __init__(self, path: str):
        super().__init__("PHOTON-FILE-102", f"Permission denied: {path}")


# Decode errors
class DecodeError(PhotonError):
    """Failed to decode image."""

    def __init__(self, path: str, reason: str = ""):
        msg = f"Failed to decode image: {path}"
        if reason:
            msg += f" ({reason})"
        super().__init__("PHOTON-DEC-101", msg)


class UnsupportedFormatError(PhotonError):
    """Unsupported image format."""

    def __init__(self, format_name: str):
        super().__init__("PHOTON-DEC-102", f"Unsupported format: {format_name}")


# Encode errors
class EncodeError(PhotonError):
    """Failed to encode image."""

    def __init__(self, path: str, reason: str = ""):
        msg = f"Failed to encode image: {path}"
        if reason:
            msg += f" ({reason})"
        super().__init__("PHOTON-ENC-101", msg)


# Automation errors
class UnknownCommandError(PhotonError):
    """Unknown automation command."""

    def __init__(self, cmd: str):
        super().__init__("PHOTON-CTL-201", f"Unknown command: {cmd}")


class InvalidArgumentsError(PhotonError):
    """Invalid command arguments."""

    def __init__(self, cmd: str, reason: str):
        super().__init__("PHOTON-CTL-202", f"Invalid arguments for {cmd}: {reason}")


class CommandTimeoutError(PhotonError):
    """Command timeout."""

    def __init__(self, cmd: str, timeout_s: float):
        super().__init__("PHOTON-CTL-203", f"Command timeout: {cmd} ({timeout_s}s)")


class FeatureDisabledError(PhotonError):
    """Feature is temporarily hidden by a feature switch."""

    def __init__(self, feature: str):
        super().__init__(
            "PHOTON-CTL-204",
            f"Feature temporarily disabled: {feature}",
        )


# UI errors
class WidgetNotFoundError(PhotonError):
    """Widget not found by objectName."""

    def __init__(self, name: str):
        super().__init__("PHOTON-UI-301", f"Widget not found: {name}")


class ConditionTimeoutError(PhotonError):
    """Wait condition timeout."""

    def __init__(self, condition: str, timeout_s: float):
        super().__init__("PHOTON-UI-302", f"Condition timeout: {condition} ({timeout_s}s)")
