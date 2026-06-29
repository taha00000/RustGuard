"""Tests for the Welch t-test TVLA core."""
import numpy as np

from tvla import THRESHOLD, welch_t


def test_welch_t_known_value():
    # fixed class {0,2}: mean 1, var 2 (ddof=1); random class {10,12}: mean 11, var 2
    # t = (1 - 11) / sqrt(2/2 + 2/2) = -10 / sqrt(2) = -7.0710678
    traces = np.array([[0.0], [2.0], [10.0], [12.0]])
    labels = np.array([0, 0, 1, 1], dtype=np.uint8)
    t = welch_t(traces, labels)
    assert t.shape == (1,)
    assert np.isclose(t[0], -10.0 / np.sqrt(2.0))


def test_constant_class_is_nan_not_inf():
    # zero variance in both classes -> denom 0 -> guarded to NaN (not a spurious peak)
    traces = np.array([[5.0], [5.0], [9.0], [9.0]])
    labels = np.array([0, 0, 1, 1], dtype=np.uint8)
    t = welch_t(traces, labels)
    assert np.isnan(t[0])


def _synth(rng, n, samples, leak):
    labels = np.tile([0, 1], n // 2).astype(np.uint8)
    traces = rng.normal(0.0, 1.0, size=(n, samples)).astype(np.float32)
    if leak:
        win = slice(samples // 2, samples // 2 + samples // 10)
        traces[labels == 1, win] += 1.0
    return traces, labels


def test_leaky_control_trips_threshold():
    rng = np.random.default_rng(1)
    tr, lab = _synth(rng, 2000, 200, leak=True)
    assert np.nanmax(np.abs(welch_t(tr, lab))) > THRESHOLD


def test_constant_time_does_not_trip():
    rng = np.random.default_rng(2)
    tr, lab = _synth(rng, 2000, 200, leak=False)
    assert np.nanmax(np.abs(welch_t(tr, lab))) < THRESHOLD
