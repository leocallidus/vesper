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
