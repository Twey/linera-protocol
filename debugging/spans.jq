#!/usr/bin/env -S jq -cRf

def to_entries_sorted:
  [keys[] as $k | {key: $k, value: .[$k]}]
  ;

def span_fields: .
  | del(.name)
  | to_entries_sorted
  | map("\(.key)=\(.value)")
  | join(",")
  ;

def span_name: .
  | [.spans[], .span]
  | map("\(.name){\(span_fields)}")
# | join(":")
  ;

fromjson?
  | select(.fields.message == "new" or .fields.message == "close")
  | { timestamp, type: .fields.message, span: span_name }
