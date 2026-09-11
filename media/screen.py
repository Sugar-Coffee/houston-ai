#!/usr/bin/env python3
"""Reconstruct the final terminal frame from a raw pty capture.

Stripping escapes and grepping does not work: ratatui only re-emits the cells
that changed, so text arrives split around cursor moves. This applies the
cursor positioning and prints what is actually on screen.
"""
import re, sys

rows, cols = int(sys.argv[2]) if len(sys.argv) > 2 else 40, int(sys.argv[3]) if len(sys.argv) > 3 else 150
data = open(sys.argv[1], 'rb').read().decode('utf-8', 'replace')

grid = [[' '] * cols for _ in range(rows)]
r = c = 0
i = 0
csi = re.compile(r'\x1b\[([0-9;?]*)([a-zA-Z])')

while i < len(data):
    ch = data[i]
    if ch == '\x1b':
        m = csi.match(data, i)
        if m:
            params, final = m.group(1), m.group(2)
            if final == 'H':
                nums = [int(p) if p else 1 for p in params.split(';')] or [1, 1]
                r = (nums[0] if nums else 1) - 1
                c = (nums[1] if len(nums) > 1 else 1) - 1
            elif final == 'J' and params in ('2', ''):
                grid = [[' '] * cols for _ in range(rows)]
            i = m.end(); continue
        # OSC and other escapes: skip to the terminator.
        j = data.find('\x07', i)
        k = data.find('\x1b\\', i)
        ends = [x for x in (j, k) if x != -1]
        i = (min(ends) + 1) if ends else i + 2
        continue
    if ch == '\r':
        c = 0; i += 1; continue
    if ch == '\n':
        r += 1; c = 0; i += 1; continue
    if ch in '\x00\x07\x0f':
        i += 1; continue
    if 0 <= r < rows and 0 <= c < cols:
        grid[r][c] = ch
    c += 1
    i += 1

for line in grid:
    print(''.join(line).rstrip())
