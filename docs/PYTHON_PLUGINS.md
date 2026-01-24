# Python Plugins (Pattern Scripts)

Vesper supports a **Python script mode** so you can create community patterns **without recompiling**.

## Enable

1. Open **Settings → Content**.
2. Select mode **“Python скрипт”**.
3. Choose a `.py` file (any location).
4. Start the screensaver.

Requirements:
- `python3` (or `python`) must be available in `PATH`.

## Script API

Your script is loaded as a module and must define:

- `render(ctx)` *(required)* → returns `bytes`/`bytearray`/`memoryview` with **RGBA** pixels, or returns `None` (then `ctx.buffer` is used).
- `init(ctx)` *(optional)* → called once, before the first frame.
- `on_resize(ctx)` *(optional)* → called when the render size changes (also recreates `ctx.buffer`).

### `ctx` fields

- `ctx.width`, `ctx.height` *(int)*: current render size (pixels)
- `ctx.time` *(float)*: seconds since start
- `ctx.dt` *(float)*: delta time (seconds)
- `ctx.frame` *(int)*: frame counter
- `ctx.seed` *(int)*: stable random seed for this run
- `ctx.mouse_x`, `ctx.mouse_y` *(float)*: mouse position (pixels)
- `ctx.mouse_down` *(bool)*: `True` when there was mouse motion recently
- `ctx.theme` *(int)*: UI theme hint (0..4)
- `ctx.quality` *(float)*: render scale hint (0.5 / 0.75 / 1.0, based on density)
- `ctx.buffer` *(bytearray)*: pre-allocated RGBA buffer (`width * height * 4` bytes)

### Output format

Pixels are **RGBA**, 8-bit per channel:

`len(buffer) == ctx.width * ctx.height * 4`

## Example script

Save as `example.py` and select it in the settings:

```python
import math


def render(ctx):
    w, h = ctx.width, ctx.height
    buf = ctx.buffer
    t = ctx.time

    # Simple animated gradient
    for y in range(h):
        fy = y / max(1, h - 1)
        for x in range(w):
            fx = x / max(1, w - 1)
            r = int(255 * fx)
            g = int(255 * fy)
            b = int(128 + 127 * math.sin(t + fx * 6.0))
            i = (y * w + x) * 4
            buf[i + 0] = r
            buf[i + 1] = g
            buf[i + 2] = b
            buf[i + 3] = 255

    return None  # use ctx.buffer
```

## Notes / troubleshooting

- Python runs in a separate process. Errors are printed to the app log (stderr).
- If your script is slow, reduce work per pixel or use `ctx.quality` to render cheaper visuals.
- Scripts are **not sandboxed**: they can execute arbitrary code as your user.

