#!/usr/bin/env python3
import importlib.util
import json
import struct
import sys
import time
import traceback


MAGIC_FRAME = b"RSFR"
MAGIC_ERROR = b"RSER"


def _send(magic: bytes, payload: bytes) -> None:
    sys.stdout.buffer.write(magic + struct.pack("<I", len(payload)) + payload)
    sys.stdout.buffer.flush()


class Context:
    __slots__ = (
        "width",
        "height",
        "time",
        "dt",
        "frame",
        "seed",
        "mouse_x",
        "mouse_y",
        "mouse_down",
        "theme",
        "quality",
        "buffer",
    )

    def __init__(self) -> None:
        self.width = 0
        self.height = 0
        self.time = 0.0
        self.dt = 0.0
        self.frame = 0
        self.seed = 0
        self.mouse_x = 0.0
        self.mouse_y = 0.0
        self.mouse_down = False
        self.theme = 0
        self.quality = 1.0
        self.buffer = bytearray()


def _load_plugin(path: str):
    spec = importlib.util.spec_from_file_location("rs_screensaver_plugin", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load plugin: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    if len(sys.argv) < 2:
        _send(MAGIC_ERROR, b"Missing plugin path argument")
        return 2

    plugin_path = sys.argv[1]
    try:
        plugin = _load_plugin(plugin_path)
    except Exception:
        _send(MAGIC_ERROR, traceback.format_exc().encode("utf-8", errors="replace"))
        return 2

    render = getattr(plugin, "render", None)
    if not callable(render):
        _send(
            MAGIC_ERROR,
            b"Plugin must define: render(ctx) -> bytes-like or None",
        )
        return 2

    init = getattr(plugin, "init", None)
    on_resize = getattr(plugin, "on_resize", None)

    ctx = Context()
    if callable(init):
        try:
            init(ctx)
        except Exception:
            _send(MAGIC_ERROR, traceback.format_exc().encode("utf-8", errors="replace"))
            # Continue anyway; user might not need init.

    last_w = 0
    last_h = 0

    for line in sys.stdin.buffer:
        if not line:
            break
        try:
            msg = json.loads(line.decode("utf-8", errors="replace"))
        except Exception:
            continue

        cmd = msg.get("cmd")
        if cmd == "quit":
            break
        if cmd != "frame":
            continue

        w = int(msg.get("w", 0) or 0)
        h = int(msg.get("h", 0) or 0)
        if w <= 0 or h <= 0:
            _send(MAGIC_FRAME, b"")
            continue

        if w != last_w or h != last_h:
            ctx.width = w
            ctx.height = h
            ctx.buffer = bytearray(w * h * 4)
            last_w = w
            last_h = h
            if callable(on_resize):
                try:
                    on_resize(ctx)
                except Exception:
                    _send(
                        MAGIC_ERROR,
                        traceback.format_exc().encode("utf-8", errors="replace"),
                    )

        ctx.time = float(msg.get("t", 0.0) or 0.0)
        ctx.dt = float(msg.get("dt", 0.0) or 0.0)
        ctx.frame = int(msg.get("frame", 0) or 0)
        ctx.seed = int(msg.get("seed", 0) or 0)
        ctx.mouse_x = float(msg.get("mx", 0.0) or 0.0)
        ctx.mouse_y = float(msg.get("my", 0.0) or 0.0)
        ctx.mouse_down = bool(msg.get("md", False))
        ctx.theme = int(msg.get("theme", 0) or 0)
        ctx.quality = float(msg.get("q", 1.0) or 1.0)

        try:
            out = render(ctx)
            if out is None:
                data = ctx.buffer
            else:
                data = out
            b = bytes(data)
            if len(b) != w * h * 4:
                raise ValueError(
                    f"render(ctx) returned {len(b)} bytes, expected {w*h*4} (RGBA)"
                )
            _send(MAGIC_FRAME, b)
        except Exception:
            _send(MAGIC_ERROR, traceback.format_exc().encode("utf-8", errors="replace"))
            # On error, keep going; next frames might succeed.
            time.sleep(0.05)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

