# Suggested Screensaver Pattern Presets (10)

These are ideas for new entries for **Content -> Patterns** (animated, purely generated visuals).
They are intended to be implementable with the current stack (GTK4 + Cairo draw loop).

1. **Perlin / Value Noise Flowfield**
   - Soft drifting noise with a subtle directional “wind”.
   - Can drive small particles or line segments for a premium look. [DONE]

2. **Aurora Bands**
   - Layered translucent ribbons moving slowly, with gentle color shifts.
   - Looks great on large screens; low distraction. [DONE]

3. **Plasma / Metaballs**
   - Classic metaball blobs or smooth plasma shader-like gradients.
   - Can be done by sampling a field on a coarse grid and interpolating. [DONE]

4. **Bokeh Drift**
   - Out-of-focus circles drifting/parallaxing, occasional slow fades.
   - Adjustable density + size range. [DONE]

5. **Particle Constellations**
   - Points moving with velocity; connect lines when near.
   - Similar vibe to “network” animations, but calmer if tuned right. [DONE]

6. **Lissajous Curves / Spirograph**
   - One or multiple parametric curves with slow phase changes.
   - Can fade trails for a long-exposure effect. [DONE]

7. **Waves (Sine Interference)**
   - Multiple sine waves (horizontal or radial) interfering.
   - Good for minimal monochrome themes. [DONE]

8. **Voronoi Cells (Animated Seeds)**
   - Voronoi diagram with seeds moving slowly; cells change over time.
   - Can render as outlines or filled regions with subtle gradients. [DONE]

9. **Scanline / CRT Grid**
   - Subtle moving scanlines + vignette + minor jitter.
   - Works well as a “retro” option, especially with a clock overlay. [DONE]

10. **Fireflies**
   - Small glowing dots wandering; occasionally brighten (pulse) like fireflies.
   - Nice nighttime ambience; easy to keep CPU use low. [DONE]

Optional Pattern Settings (Worth Exposing Later)
- Speed: slow/normal/fast [DONE]
- Density: low/medium/high [DONE]
- Color theme: mono / warm / cool / random palette
- Trail / persistence: off / short / long
