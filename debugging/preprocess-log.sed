#!/usr/bin/env -S sed -nf
 s/^\([[:digit:]:.]*\)[^{]*{\(.*\)}/{ "timestamp": "\1", \2 }/p
