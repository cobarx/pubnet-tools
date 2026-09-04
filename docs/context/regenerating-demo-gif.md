# Regenerating the Demo GIF

The GIF at the top of the README (`docs/assets/demo.gif`) is a real `pubnetchk -q -v`
run, captured with [asciinema](https://asciinema.org/) and rendered with
[agg](https://github.com/asciinema/agg) (both installable via `cargo install`, no
system packages needed).

```bash
asciinema rec --window-size 100x24 --idle-time-limit 1 \
  -c "./target/release/pubnetchk -q -v" demo-raw.cast

# drop redundant spinner frames instead of speeding up playback —
# see scripts/trim-demo-cast.py for what "redundant" means here
python3 scripts/trim-demo-cast.py demo-raw.cast demo-trimmed.cast

agg --theme github-dark --font-size 15 --rows 24 --last-frame-duration 11.6 demo-trimmed.cast docs/assets/demo.gif
```

Redact anything network-identifying (your real SSID, in particular) out of the
`.cast` file before rendering — it's plain JSON, a text substitution is enough.
