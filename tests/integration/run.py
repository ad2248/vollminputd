#!/usr/bin/env python3
"""Single entry point; all orchestration lives in pytest fixtures."""
import sys
from pathlib import Path

try:
    import pytest
except ImportError:
    sys.exit("Install pytest for this Python interpreter before running container tests.")

if __name__ == "__main__":
    sys.exit(pytest.main([str(Path(__file__).resolve().parent), *sys.argv[1:]]))
