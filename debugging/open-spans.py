#!/usr/bin/env python3

import fileinput
import json

from collections import defaultdict

opened = defaultdict(lambda: 0)

try:
    for line in fileinput.input():
        obj = json.loads(line)
        if obj['type'] == "new":
            opened[tuple(obj['span'])] += 1
        else:
            opened[tuple(obj['span'])] -= 1
except BrokenPipeError:
    pass

open = {}

for k, n in opened.items():
    if n > 0:
        open.push(k)
