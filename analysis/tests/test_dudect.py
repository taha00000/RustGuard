"""Tests for the dudect timing-leakage core."""
import numpy as np

from dudect import THRESHOLD, welch_scalar


def test_welch_known_value():
    # fixed {0,2}: mean 1, var 2 (ddof=1); random {10,12}: mean 11, var 2
    # t = (1 - 11) / sqrt(2/2 + 2/2) = -10 / sqrt(2)
    fixed = np.array([0.0, 2.0])
    rand = np.array([10.0, 12.0])
    assert np.isclose(welch_scalar(fixed, rand), -10.0 / np.sqrt(2.0))


def test_deterministic_identical_is_zero():
    # constant-time, perfectly deterministic, identical timing -> t = 0, no leak
    fixed = np.full(100, 4200.0)
    rand = np.full(100, 4200.0)
    assert welch_scalar(fixed, rand) == 0.0


def test_deterministic_different_is_inf():
    # constant but different timing -> definite (infinite) separation
    fixed = np.full(100, 4200.0)
    rand = np.full(100, 4180.0)
    assert np.isinf(welch_scalar(fixed, rand))


def _synth(rng, n, leak):
    labels = np.tile([0, 1], n // 2).astype(np.uint8)
    cyc = rng.normal(4200.0, 2.0, size=n)
    if leak:
        cyc[labels == 1] = rng.normal(4180.0, 8.0, size=int((labels == 1).sum()))
    return cyc, labels


def test_leaky_trips_threshold():
    rng = np.random.default_rng(1)
    cyc, lab = _synth(rng, 20000, leak=True)
    assert abs(welch_scalar(cyc[lab == 0], cyc[lab == 1])) > THRESHOLD


def test_constant_time_does_not_trip():
    rng = np.random.default_rng(2)
    cyc, lab = _synth(rng, 20000, leak=False)
    assert abs(welch_scalar(cyc[lab == 0], cyc[lab == 1])) < THRESHOLD
