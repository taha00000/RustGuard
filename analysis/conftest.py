"""Make the analysis modules importable from tests/ regardless of cwd.

Its mere presence puts this directory on sys.path under pytest's default import
mode; the explicit insert keeps it robust across pytest versions and when run
from the repo root.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
