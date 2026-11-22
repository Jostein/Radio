#!/bin/sh
# A one-line now-playing module for Waybar. Needs playerctl.
# The app publishes MPRIS as org.mpris.MediaPlayer2.radio.

status=$(playerctl -p radio status 2>/dev/null) || exit 0

case "$status" in
  Playing) icon="▶" ;;
  Paused)  icon="⏸" ;;
  *)       icon="■" ;;
esac

artist=$(playerctl -p radio metadata artist 2>/dev/null)
title=$(playerctl -p radio metadata title 2>/dev/null)

if [ -n "$artist" ] && [ -n "$title" ] && [ "$artist" != "$title" ]; then
  printf '%s %s — %s\n' "$icon" "$artist" "$title"
elif [ -n "$title" ]; then
  printf '%s %s\n' "$icon" "$title"
elif [ -n "$artist" ]; then
  printf '%s %s\n' "$icon" "$artist"
else
  printf '%s Radio\n' "$icon"
fi
