#!/usr/bin/env bash
cat log.log | ./preprocess-log.sed | ./spans.jq | ./open-spans.py
