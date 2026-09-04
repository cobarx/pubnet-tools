#!/usr/bin/env python3
"""Trim redundant spinner frames out of an asciicast recording.

pubnetchk's spinner repaints every ~80ms while checks run, which is
realistic but makes for a long, boring demo GIF. This drops the *frames*
in the middle of any long spinner run (keeping a few at the start and end
so the motion still reads as "it's working"), rather than speeding up
playback — the frames that remain keep their original recorded timing.

Usage: trim-demo-cast.py <in.cast> <out.cast> [--keep-start N] [--keep-end N]
"""

import argparse
import json
import re
import sys

SPINNER_RE = re.compile(r"\r\x1b\[2K[⠀-⣿]")


def is_spinner_frame(event):
    _delta, kind, data = event
    return kind == "o" and bool(SPINNER_RE.search(data))


def trim(events, keep_start, keep_end):
    out = []
    run = []

    def flush_run():
        if len(run) <= keep_start + keep_end:
            out.extend(run)
            return
        kept = run[:keep_start] + run[-keep_end:]
        # First frame after the cut shouldn't carry the accumulated gap —
        # give it a normal spinner-tick delta instead of a visible pause.
        if keep_end > 0 and keep_start > 0:
            normal_tick = run[keep_start][0]
            kept[keep_start] = [normal_tick, kept[keep_start][1], kept[keep_start][2]]
        out.extend(kept)

    for event in events:
        if is_spinner_frame(event):
            run.append(event)
        else:
            flush_run()
            run = []
            out.append(event)
    flush_run()
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--keep-start", type=int, default=22)
    parser.add_argument("--keep-end", type=int, default=22)
    args = parser.parse_args()

    with open(args.input) as f:
        lines = f.readlines()

    header = lines[0]
    events = [json.loads(line) for line in lines[1:] if line.strip()]
    before = len(events)
    trimmed = trim(events, args.keep_start, args.keep_end)
    after = len(trimmed)

    with open(args.output, "w") as f:
        f.write(header)
        for event in trimmed:
            f.write(json.dumps(event, separators=(",", ":")) + "\n")

    print(f"{before} -> {after} events ({before - after} spinner frames dropped)", file=sys.stderr)


if __name__ == "__main__":
    main()
