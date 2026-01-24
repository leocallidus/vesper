# User GLSL / Shadertoy shaders

Vesper can run **user-provided GLSL fragment shaders** (Shadertoy-style) **without recompiling**.

## Enable

1. Open **Settings → Content**
2. Select mode **“GLSL шейдер”**
3. Choose a shader file (`.glsl`, `.frag`, `.fs`)
4. Start the screensaver

Performance note:
- **Pattern density** controls internal render scale for shaders (Low/Medium renders at lower resolution and upscales for better FPS).

## Multi-pass shaders (up to 5 files)

Multi-pass Shadertoy setups are supported in this format:

- `Common.glsl` (optional) — shared code (functions/constants) included into all passes
- `Image.glsl` (required) — final pass
- `BufferA.glsl` / `BufferB.glsl` / `BufferC.glsl` / `BufferD.glsl` (optional)

Put the files in the same folder and select **`Image.glsl`** in the settings.
The engine will auto-detect buffers next to it and run up to **4 buffers + Image**.

Notes:
- `iChannel0..3` are mapped as `BufferA..BufferD`.
- Buffers support simple feedback (buffer sampling itself uses the previous frame).
- `Common.glsl` should be a snippet (no `#version` line).

## Supported format (Shadertoy-style)

Your file should contain the Shadertoy function:

```glsl
void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
    // ...
}
```

Vesper automatically wraps it into a `#version 330 core` shader and calls:

```glsl
mainImage(fragColor, gl_FragCoord.xy);
```

## Supported uniforms

These Shadertoy uniforms are provided:

- `uniform vec3  iResolution;`  // (width, height, pixelAspect=1)
- `uniform float iTime;`        // seconds since start
- `uniform float iTimeDelta;`   // delta time (seconds)
- `uniform int   iFrame;`       // frame counter
- `uniform vec4  iMouse;`       // (x, y, z, w). Only x/y are filled for now
- `uniform vec4  iDate;`        // (year, month, day, seconds)

Texture channels exist but are currently **dummy black textures** (unless the channel is backed by a BufferA-D output):

Notes:
- By default, channels are treated as `sampler2D`.
- If a shader tries to sample a channel as `texture(iChannelN, vec3(...))` (cubemap-style),
  Vesper will automatically recompile that pass with `iChannelN` declared as a `samplerCube`
  to match Shadertoy behavior.
- Cubemap/3D channels are currently provided as **black 1×1 fallbacks** (so the shader compiles and runs,
  but visuals may differ from Shadertoy if it relies on external textures).

Also available for compatibility:

```glsl
#define texture2D texture
#define textureCube texture
```

## Example

Save as `retro_sun.glsl` and select it in the settings:

```glsl
// Original: https://www.shadertoy.com/view/3t3GDB
// Original License: CC BY 3.0
// Original Author: Jan Mróz (jaszunio15)

float sdSkyscraper(vec2 p, float w, float h)
{
  vec2 k1 = vec2(0.0, h);
  vec2 k2 = vec2(-w, h);
  p.x = abs(p.x);
  vec2 ca = p - vec2(0.0, h);
  vec2 cb = p - k1 + k2;
  float s = (cb.x < 0.0 && ca.y < 0.0) ? - 1.0 : 1.0;
  return s * (dot(ca, ca));
}

float sun(vec2 uv)
{
  float val = smoothstep(0.7, 0.69, length(uv));
  float bloom = smoothstep(0.7, 0.0, length(uv));
  float cut = 5.0 * sin((uv.y + iTime * 0.2) * 60.0)
    + clamp(uv.y * 15.0, -6.0, 6.0);
  cut = clamp(cut, 0.0, 1.0);
  return clamp(val * cut, 0.0, 1.0) + bloom * 0.6;
}

float grid(vec2 uv)
{
  vec2 size = vec2(uv.y, uv.y * uv.y * 0.2) * 0.01;
  uv += vec2(0.0, iTime * 4.0 * 1.05);
  uv = abs(fract(uv) - 0.5);
  vec2 lines = smoothstep(size, vec2(0.0), uv);
  lines += smoothstep(size * 5.0, vec2(0.0), uv) * 0.4;
  return clamp(lines.x + lines.y, 0.0, 3.0);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord)
{
  vec2 uv = (2.0 * fragCoord.xy - iResolution.xy) / iResolution.y;

  float fog = smoothstep(0.2, -0.05, abs(uv.y + 0.2));
  vec3 col = vec3(0.0, 0.1, 0.2);

  if (uv.y < -0.2)
  {
    uv.y = 3.0 / (abs(uv.y + 0.2) + 0.05);
    uv.x *= uv.y;
    float gridVal = grid(uv);
    col = mix(col, vec3(1.0, 0.25, 0.5), gridVal);
  }
  else
  {
    uv.y -= 0.34;
    col = vec3(1.0, 0.4, 0.4);
    float sunVal = sun(uv);
    col = mix(col, vec3(1.0, 0.85, 0.3), uv.y * 2.5 + 0.2);
    col = mix(vec3(0.0), col, sunVal);

    uv.y -= 0.2;
    float bldgD = max(-uv.y * 1.2 + 0.18, 0.0);
    float b1 = sdSkyscraper(uv + vec2(0.1 * mod(iTime, 40.0) - 2.0, 0.5), 0.1, 0.15);
    col = mix(col, mix(vec3(0.0, 0.0, 0.25), vec3(1.0, 0.0, 0.5), bldgD), step(b1, 0.0));
  }

  col += fog * fog * fog;
  fragColor = vec4(col, 1.0);
}
```

## Troubleshooting

- Shader compile/link errors are printed to stderr as `Shadertoy GL init failed: ...`.
- If your shader uses `iChannel0..3` and you don’t provide buffers, it will sample black (channels are placeholders).
- User shaders are not sandboxed: they run on your GPU driver and can crash the GPU process on buggy drivers.
  Configure **“Принудительное закрытие”** hotkey in Settings → General for emergency exit.
