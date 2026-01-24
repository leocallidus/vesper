# Future Features & Roadmap for RS-Screensaver

This document outlines planned features and architectural improvements to elevate `vesper` from a basic utility to a comprehensive screen locking and ambient display system.

## 1. Core Architecture & Rendering

### 🚀 GPU Acceleration (Crucial) [DONE]
Currently, all patterns use `Cairo` (CPU rendering). While optimized, this limits resolution (4K+) and complexity (fluid simulations).
- **Implementation:** Migrate pattern rendering to **`gtk4::GLArea`**.
- **Benefit:** 60+ FPS on 4K screens, "Shadertoy"-style fluid simulations, 3D patterns, near-zero CPU usage. 

### 🔒 Native Wayland Locking [DONE]
Currently, on Wayland, the app creates a fullscreen window. It does not strictly *secure* the session against bypassing (e.g., switching TTY or crashing the shell).
- **Implementation:** Implement the **`ext-session-lock-v1`** Wayland protocol.
- **Benefit:** Functions as a true security barrier (replacement for `swaylock`/`hyprlock`).

### 🔋 Battery Awareness [DONE]
- **Feature:** Detect laptop battery state (via UPower).
- **Behavior:** Automatically switch to a "Black screen" when on battery power to save energy.

## 2. Advanced Content Modes

### 🖼️ Enhanced Media [PARTIALLY DONE]
- **Ken Burns Effect:** Pan and zoom animation for static images (slideshows). [HARD TO REALIZATION]
- **Cinemagraphs:** Support for seamless looping high-res video backgrounds without stutters. [DONE]
- **Filters:** Apply sepia, grayscale and blur filters over user wallpapers. It can be enabled or disabled for each effect. [NOT NEEDED]

### 🌐 Live Data Widgets [PARTIALLY DONE]
- **Weather:** Current temperature and icon (via OpenWeatherMap or localized system provider). [good feature but I dont have money to bill api key and payment difficulties]
- **Now Playing:** Album art and track info from media players (MPRIS integration already exists for pausing; extend it to *display* info). [DONE]
- **RSS/News Ticker:** Scrolling text from user-defined RSS feeds.  [DONE] 
- **System Stats:** CPU/RAM usage graphs (retro-tech style). [DONE]

### 🖥️ Web & HTML [DONE]
- **Interactive Web:** Allow mouse interaction with the "Web" mode (optional toggle) for interactive HTML5 wallpapers.

## 3. Configuration & UX

### 📅 Scheduler / Night Mode [NOT NEEDED]
- **Time-based Profiles:** Automatically switch to a "Red tint" or dimmer profile after sunset.
- **Work/Home Modes:** Detect network (SSID) to switch profiles.

### 🖱️ Interactive Preview [NOT NEEDED]
- **Live Edit:** Changing settings (colors, speed) in the preferences window should update the preview instantly without needing a "Save" (already partially implemented, but could be smoother).

### ⌨️ Input Handling [NOT NOW]
- **Unlock Dialog:** Instead of just closing on mouse move, show a password/fingerprint unlock dialog (requires PAM integration + Wayland Lock protocol).

## 4. New Pattern Ideas

### 🌊 Fluid Dynamics [DONE]
- **Smoke/Ink:** Interactive smoke that reacts to mouse movement (requires GPU). 
- **Water Ripples:** Realistic water distortion over a background image. 

### 👾 Retro / Sci-Fi [DONE]
- **Matrix Rain 2.0:** 3D projected glyphs.
- **Star Trek LCARS:** Functional-looking sci-fi interface.
- **Terminal:** Replay logs or fake "hacking" text sequences.

### 🎨 Abstract [DONE]
- **Fractals:** Julia/Mandelbrot sets with zoom.
- **Reaction-Diffusion:** Gray-Scott organic pattern growth.

## 5. Plugin System [DONE]
- **Python Scripting:** Allow users to write simple scripts to draw to the canvas, enabling community-created patterns without recompiling the app.
