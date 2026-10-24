#!/usr/bin/env python3
"""Stand-in for the Obscura CLI, used ONLY by the test suite.

It mimics the slice of the real command line that ``agent_tools.browser``
depends on, so the wrapper's plumbing (argument construction, JSON decoding,
exit-code handling, timeout killing) can be tested without a 70 MB binary or
network access.

This is NOT Obscura. Passing these tests says nothing about whether real
pages render; only a live run with the official binary does that.
"""

import json
import sys
import time

VERSION = "obscura 0.2.5"

EVAL_RESULTS = {
    "document.title": "Example Domain",
    "1+1": 2,
    "document.querySelectorAll('a').length": 1,
}


def main(argv):
    if "--version" in argv or "-V" in argv:
        print(VERSION)
        return 0

    if "fetch" not in argv:
        print(f"error: unrecognized subcommand: {argv}", file=sys.stderr)
        return 2

    index = argv.index("fetch")
    url = argv[index + 1] if index + 1 < len(argv) else ""

    if "hang" in url:
        time.sleep(60)
        return 0
    if "fail" in url:
        print(f"error: failed to resolve host in {url}: name or service not known", file=sys.stderr)
        return 1

    if "--eval" in argv:
        expression = argv[argv.index("--eval") + 1]
        print(json.dumps(EVAL_RESULTS.get(expression)))
        return 0

    if "--screenshot" in argv:
        if "noshot" in url:
            return 0  # simulates a non-render build: exits 0, writes nothing
        destination = argv[argv.index("--screenshot") + 1]
        with open(destination, "wb") as handle:
            handle.write(b"\x89PNG\r\n\x1a\n")
        return 0

    if "--dump" in argv:
        fmt = argv[argv.index("--dump") + 1]
        print(f"[{fmt}] Example Domain — illustrative body text")
        return 0

    print("<html><title>Example Domain</title></html>")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
