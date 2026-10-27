#!/usr/bin/env python3
"""Smoke test of the web app: loads it in headless Chromium, moves the pointer and clicks for a
while, and fails on a Rust panic in the page.

In the browser (wasm32-unknown-unknown) a panic kills the app for good: the page keeps its last
frame and every click after it throws. std's clocks (`Instant::now`, `SystemTime::now`) panic
there, and the tests can't see that, since they run natively; this runs the real build.

    python3 packaging/web/smoke.py URL [SECONDS]

URL is the built site (`trunk build --release`, served with `python3 -m http.server`).
Needs Playwright for Python and its Chromium (`CHROMIUM=path` uses another Chromium).
"""

import os
import sys
import time

from playwright.sync_api import sync_playwright

PANIC = ("panicked", "time not implemented", "RuntimeError", "unreachable")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    url = sys.argv[1]
    seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 40.0
    bad: list[str] = []
    with sync_playwright() as pw:
        browser = pw.chromium.launch(
            executable_path=os.environ.get("CHROMIUM") or None,
            args=["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"]
        )
        page = browser.new_page(viewport={"width": 1280, "height": 800})
        page.on("console", lambda m: bad.append(m.text[:400]) if any(p in m.text for p in PANIC) else None)
        page.on("pageerror", lambda e: bad.append(f"page error: {str(e)[:400]}"))
        page.goto(url, wait_until="load", timeout=180_000)
        # the app draws once the module has started
        page.wait_for_selector("canvas", timeout=180_000)
        end = time.monotonic() + seconds
        moves = 0
        while time.monotonic() < end and not bad:
            page.mouse.move(200 + (moves * 37) % 800, 150 + (moves * 53) % 500)
            if moves % 25 == 10:
                page.mouse.click(640, 400)
            page.wait_for_timeout(100)
            moves += 1
        browser.close()
    if bad:
        print("The web app panicked:")
        for line in bad[:5]:
            print("  " + line)
        return 1
    print(f"ok: no panic in {seconds:.0f} s ({moves} pointer moves)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
