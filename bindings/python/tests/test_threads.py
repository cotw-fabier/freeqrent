"""Renders release the GIL: threads run in parallel and agree."""

import os
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from typing import List

import pytest

import freeqrent as fq

HEAVY = fq.Style(preset="bubble", intro="ripple", encoding=fq.Encoding(version=25))
PAYLOADS = [f"https://example.com/{i}" for i in range(32)]


def test_threads_agree_with_a_serial_run() -> None:
    serial = [fq.render(p, HEAVY) for p in PAYLOADS]
    with ThreadPoolExecutor(max_workers=8) as pool:
        threaded = list(pool.map(lambda p: fq.render(p, HEAVY), PAYLOADS))
    assert threaded == serial
    assert len(set(serial)) == len(serial)


def test_python_runs_while_a_render_does() -> None:
    # Held, the GIL would stop this thread's counting for the whole render.
    long_style = fq.Style(preset="bubble", intro="ripple", encoding=fq.Encoding(version=40))
    done = threading.Event()
    counts: List[int] = []

    def count() -> None:
        n = 0
        while not done.is_set():
            n += 1
        counts.append(n)

    start = time.perf_counter()
    for _ in range(5):
        fq.render("https://example.com", long_style)
    one = (time.perf_counter() - start) / 5

    t = threading.Thread(target=count)
    t.start()
    time.sleep(0.01)
    before = time.perf_counter()
    fq.render("https://example.com", long_style)
    inside = time.perf_counter() - before
    done.set()
    t.join()
    assert inside >= one * 0.5, (inside, one)
    assert counts and counts[0] > 1000


@pytest.mark.skipif((os.cpu_count() or 1) < 4, reason="needs at least 4 cores to measure")
def test_threads_render_in_parallel() -> None:
    work = PAYLOADS * 2

    def serial() -> float:
        start = time.perf_counter()
        for p in work:
            fq.render(p, HEAVY)
        return time.perf_counter() - start

    def threaded() -> float:
        with ThreadPoolExecutor(max_workers=4) as pool:
            start = time.perf_counter()
            list(pool.map(lambda p: fq.render(p, HEAVY), work))
            return time.perf_counter() - start

    serial()  # warm up
    s = min(serial() for _ in range(3))
    t = min(threaded() for _ in range(3))
    # Four threads on four or more cores; with the GIL held it would be ~1x.
    assert s / t > 1.5, f"serial {s:.3f}s, 4 threads {t:.3f}s: {s / t:.2f}x"
