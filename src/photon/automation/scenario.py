"""YAML scenario runner + assertions + screenshots."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import yaml

from photon.automation.cli import send_command
from photon.core.features import compare_enabled

#: Scenario ``requires_feature:`` values → feature switch checkers.
_FEATURES = {"compare": compare_enabled}


def _feature_enabled(feature: str) -> bool:
    """Return whether ``feature`` is currently enabled.

    Args:
        feature: Feature name used in ``requires_feature:``

    Raises:
        ValueError: When the feature name is unknown
    """
    checker = _FEATURES.get(feature)
    if checker is None:
        raise ValueError(f"unknown feature: {feature!r}")
    return checker()


class ScenarioRunner:
    """Run YAML scenario files with assertions and screenshots."""

    def __init__(self, socket_path: str, out_dir: Path | None = None) -> None:
        self.socket_path = socket_path
        self.out_dir = out_dir or Path("/tmp/photon-scenario")
        self.out_dir.mkdir(parents=True, exist_ok=True)

        self.results: list[dict[str, Any]] = []

    def run(self, scenario_path: Path) -> dict[str, Any]:
        """Run scenario file.

        Args:
            scenario_path: Path to YAML scenario

        Returns:
            Results dict with pass/fail counts
        """
        # Load scenario
        with open(scenario_path, encoding="utf-8") as f:
            scenario = yaml.safe_load(f)

        name = scenario.get("name", scenario_path.stem)
        setup = scenario.get("setup", [])
        steps = scenario.get("steps", [])
        required = scenario.get("requires_feature")

        print(f"[scenario] Running: {name}")

        # Feature switches (core/features.py) can hide a whole feature; the
        # scenario then reports "skipped" instead of failing every step.
        if required and not _feature_enabled(str(required)):
            results = {
                "scenario": name,
                "passed": 0,
                "failed": 0,
                "skipped": 1,
                "total": len(steps),
                "skip_reason": f"feature disabled: {required}",
                "steps": [],
            }
            results_path = self.out_dir / "result.json"
            with open(results_path, "w", encoding="utf-8") as f:
                json.dump(results, f, indent=2, ensure_ascii=False)
            print(f"[scenario] SKIPPED (feature disabled: {required})")
            print(f"[scenario] Saved to: {results_path}")
            return results

        print(f"[scenario] Steps: {len(steps)}")

        # Run setup
        self._run_setup(setup)

        # Run steps
        passed = 0
        failed = 0

        for i, step in enumerate(steps):
            step_result = self._run_step(i, step)
            self.results.append(step_result)

            if step_result["passed"]:
                passed += 1
                print(f"  [{i + 1}/{len(steps)}] ✓ {step_result['cmd']}")
            else:
                failed += 1
                print(f"  [{i + 1}/{len(steps)}] ✗ {step_result['cmd']}")
                for err in step_result.get("errors", []):
                    print(f"      {err}")

        # Save results
        results = {
            "scenario": name,
            "passed": passed,
            "failed": failed,
            "total": len(steps),
            "steps": self.results,
        }

        results_path = self.out_dir / "result.json"
        with open(results_path, "w", encoding="utf-8") as f:
            json.dump(results, f, indent=2, ensure_ascii=False)

        print(f"\n[scenario] Results: {passed}/{len(steps)} passed")
        print(f"[scenario] Saved to: {results_path}")

        return results

    def _run_setup(self, setup: list[dict[str, Any]]) -> None:
        """Run setup commands (local, not via socket)."""
        for item in setup:
            if "mkdir" in item:
                Path(item["mkdir"]).mkdir(parents=True, exist_ok=True)
            elif "copy" in item:
                import shutil

                src = Path(item["copy"]["src"])
                dst = Path(item["copy"]["dst"])
                if src.exists():
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(src, dst)
            elif "touch" in item:
                Path(item["touch"]).touch()
            elif "rm" in item:
                import shutil

                path = Path(item["rm"])
                if path.is_dir():
                    shutil.rmtree(path, ignore_errors=True)
                elif path.exists():
                    path.unlink()
            elif "run" in item:
                import subprocess

                subprocess.run(item["run"], shell=True, check=False)

    def _run_step(self, index: int, step: dict[str, Any]) -> dict[str, Any]:
        """Run a single step.

        Args:
            index: Step index (0-based)
            step: Step definition

        Returns:
            Step result dict
        """
        cmd = step.get("cmd", "")
        args = step.get("args", {})
        asserts = step.get("assert", [])
        expect_fail = step.get("expect_fail", False)

        result: dict[str, Any] = {
            "index": index,
            "cmd": cmd,
            "args": args,
            "passed": False,
            "errors": [],
            "response": None,
        }

        # Execute command
        try:
            response = send_command(self.socket_path, cmd, args, timeout=10.0)
            result["response"] = response

            if not response.get("ok"):
                if expect_fail:
                    result["passed"] = True
                    result["expected_failure"] = True
                else:
                    result["errors"].append(
                        f"Command failed: {response.get('error')} - {response.get('message')}"
                    )
                return result

            # Run assertions
            data = response.get("data", {})
            for assertion in asserts:
                error = self._check_assertion(assertion, data)
                if error:
                    result["errors"].append(error)

            if not result["errors"]:
                result["passed"] = True

        except Exception as e:
            if expect_fail:
                result["passed"] = True
                result["expected_failure"] = True
            else:
                result["errors"].append(f"Exception: {e}")

        return result

    def _check_assertion(self, assertion: dict[str, Any], data: dict[str, Any]) -> str | None:
        """Check a single assertion.

        Args:
            assertion: Assertion definition
            data: Response data

        Returns:
            Error message or None if passed
        """
        prop = assertion.get("property", "")

        # Scenario YAML may express the operator inline (``greater_than: 0``,
        # ``not_equals: []``) or via an explicit ``op`` key. Support both.
        operator = assertion.get("op", "")
        expected = assertion.get("equals")
        if not operator:
            operator = "equals"
            for candidate in (
                "equals",
                "not_equals",
                "greater_than",
                "less_than",
                "contains",
                "not_contains",
            ):
                if candidate in assertion:
                    operator = candidate
                    expected = assertion[candidate]
                    break

        # Get actual value (supports dot notation)
        actual = data
        for part in prop.split("."):
            if isinstance(actual, dict) and part in actual:
                actual = actual[part]
            else:
                return f"Property not found: {prop}"

        # Check assertion
        if operator == "equals":
            if actual != expected:
                return f"{prop}: expected {expected!r}, got {actual!r}"
        elif operator == "not_equals":
            if actual == expected:
                return f"{prop}: expected != {expected!r}, got {actual!r}"
        elif operator == "greater_than":
            if not isinstance(actual, (int, float)) or not isinstance(expected, (int, float)):
                return f"{prop}: greater_than requires numeric values"
            if not (actual > expected):
                return f"{prop}: expected > {expected!r}, got {actual!r}"
        elif operator == "less_than":
            if not isinstance(actual, (int, float)) or not isinstance(expected, (int, float)):
                return f"{prop}: less_than requires numeric values"
            if not (actual < expected):
                return f"{prop}: expected < {expected!r}, got {actual!r}"
        elif operator == "contains":
            if expected not in actual:
                return f"{prop}: expected to contain {expected!r}, got {actual!r}"
        elif operator == "not_contains":
            if expected in actual:
                return f"{prop}: expected NOT to contain {expected!r}, got {actual!r}"
        else:
            return f"Unknown operator: {operator}"

        return None


def run_scenario(
    scenario_path: str | Path,
    socket_path: str,
    out_dir: str | Path | None = None,
) -> dict[str, Any]:
    """Run scenario file.

    Args:
        scenario_path: Path to YAML scenario
        socket_path: Socket path
        out_dir: Output directory

    Returns:
        Results dict
    """
    runner = ScenarioRunner(
        socket_path=socket_path,
        out_dir=Path(out_dir) if out_dir else None,
    )
    return runner.run(Path(scenario_path))
