use crate::config::PatternDensity;
use glow::HasContext;
use gtk4::prelude::*;
use gtk4::{EventControllerMotion, GestureClick, GLArea};
use std::io::Write;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::rc::Rc;
use std::time::Instant;

const SHADER_CHANNELS: usize = 4;
const SOUND_SAMPLE_RATE: f32 = 44_100.0;
const SOUND_BLOCK_SAMPLES: i32 = 2048;
const SOUND_AHEAD_SECONDS: f32 = 0.5;

#[derive(Clone, Copy, Default)]
struct MouseState {
    pos: Option<(f32, f32)>,
    last_press: Option<(f32, f32)>,
    pressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SamplerKind {
    Tex2D,
    Cube,
    #[allow(dead_code)]
    Tex3D,
}

#[derive(Clone)]
struct ShadertoyProgram {
    program: glow::NativeProgram,
    uniforms: Uniforms,
    channel_kinds: [SamplerKind; SHADER_CHANNELS],
}

struct BufferState {
    fbo: glow::NativeFramebuffer,
    tex_prev: glow::NativeTexture,
    tex_next: glow::NativeTexture,
    size: (i32, i32),
}

struct ShadertoyResources {
    _libgl: libloading::Library,
    gl: glow::Context,
    image: ShadertoyProgram,
    buffers: [Option<ShadertoyProgram>; SHADER_CHANNELS],
    external_channels: [Option<ExternalChannel>; SHADER_CHANNELS],
    noise_tex: glow::NativeTexture,
    sound: Option<SoundState>,
    blit_program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    black_tex: glow::NativeTexture,
    black_cube_tex: glow::NativeTexture,
    black_3d_tex: glow::NativeTexture,
    buffer_states: [Option<BufferState>; SHADER_CHANNELS],
    fbo: glow::NativeFramebuffer,
    fbo_tex: glow::NativeTexture,
    fbo_size: (i32, i32),
    u_blit_tex: Option<glow::NativeUniformLocation>,
    start: Instant,
    last_t: f32,
    frame: i32,
}

#[derive(Clone)]
struct ExternalChannel {
    tex: glow::NativeTexture,
    size: (i32, i32),
}

struct SoundProgram {
    program: glow::NativeProgram,
    uniforms: Uniforms,
    channel_kinds: [SamplerKind; SHADER_CHANNELS],
    u_sample_rate: Option<glow::NativeUniformLocation>,
    u_block_offset: Option<glow::NativeUniformLocation>,
}

struct SoundState {
    program: SoundProgram,
    fbo: glow::NativeFramebuffer,
    tex: glow::NativeTexture,
    pixels: Vec<f32>,
    bytes: Vec<u8>,
    next_sample: i32,
    block_index: i32,
    volume: f32,
    backend: SoundBackend,
}

enum SoundBackend {
    PwCat(Child),
    PaPlay(Child),
}

impl SoundBackend {
    fn kind_name(&self) -> &'static str {
        match self {
            SoundBackend::PwCat(_) => "pw-cat",
            SoundBackend::PaPlay(_) => "paplay",
        }
    }

    fn is_pw_cat(&self) -> bool {
        matches!(self, SoundBackend::PwCat(_))
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match self {
            SoundBackend::PwCat(c) | SoundBackend::PaPlay(c) => c.try_wait(),
        }
    }

    fn stdin_mut(&mut self) -> Option<&mut ChildStdin> {
        match self {
            SoundBackend::PwCat(c) | SoundBackend::PaPlay(c) => c.stdin.as_mut(),
        }
    }

    fn kill_and_wait(&mut self) {
        let _ = match self {
            SoundBackend::PwCat(c) | SoundBackend::PaPlay(c) => c.kill(),
        };
        let _ = match self {
            SoundBackend::PwCat(c) | SoundBackend::PaPlay(c) => c.wait(),
        };
    }
}

#[allow(dead_code)]
pub fn build_shadertoy_area(
    shader_path: &str,
    density: PatternDensity,
    enable_sound: bool,
    sound_volume: f32,
) -> GLArea {
    build_shadertoy_area_inner(shader_path, density, enable_sound, sound_volume, true, true)
}

pub fn build_shadertoy_area_with_options(
    shader_path: &str,
    density: PatternDensity,
    enable_sound: bool,
    sound_volume: f32,
    enable_mouse: bool,
) -> GLArea {
    build_shadertoy_area_inner(
        shader_path,
        density,
        enable_sound,
        sound_volume,
        true,
        enable_mouse,
    )
}

pub fn build_shadertoy_thumbnail_area(shader_path: &str, width: i32, height: i32) -> GLArea {
    let area = build_shadertoy_area_inner(
        shader_path,
        PatternDensity::Low,
        false,
        0.0,
        false,
        false,
    );
    area.set_size_request(width.max(1), height.max(1));
    area.set_hexpand(false);
    area.set_vexpand(false);
    area
}

fn build_shadertoy_area_inner(
    shader_path: &str,
    density: PatternDensity,
    enable_sound: bool,
    sound_volume: f32,
    continuous_render: bool,
    enable_mouse: bool,
) -> GLArea {
    let gl_area = GLArea::new();
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);
    gl_area.set_auto_render(!continuous_render);
    gl_area.set_has_depth_buffer(false);
    gl_area.set_has_stencil_buffer(false);
    #[allow(deprecated)]
    gl_area.set_use_es(false);
    gl_area.set_required_version(3, 3);

    let shader_path = shader_path.trim().to_string();
    let sound_volume = sound_volume.clamp(0.0, 1.0);
    let resources: Rc<RefCell<Option<ShadertoyResources>>> = Rc::new(RefCell::new(None));

    let mouse_state: Rc<RefCell<MouseState>> = Rc::new(RefCell::new(MouseState::default()));
    if enable_mouse {
        let mouse_state_for_motion = mouse_state.clone();
        let gl_area_for_scale = gl_area.clone();
        let controller = EventControllerMotion::new();
        controller.connect_motion(move |_, x, y| {
            let scale = gl_area_for_scale.scale_factor().max(1) as f32;
            mouse_state_for_motion
                .borrow_mut()
                .pos
                .replace((x as f32 * scale, y as f32 * scale));
        });
        gl_area.add_controller(controller);

        let mouse_state_for_press = mouse_state.clone();
        let gl_area_for_scale = gl_area.clone();
        let click = GestureClick::new();
        click.connect_pressed(move |_, _n_press, x, y| {
            let scale = gl_area_for_scale.scale_factor().max(1) as f32;
            let mut state = mouse_state_for_press.borrow_mut();
            state.pressed = true;
            state.last_press = Some((x as f32 * scale, y as f32 * scale));
        });
        let mouse_state_for_release = mouse_state.clone();
        click.connect_released(move |_, _n_press, _x, _y| {
            mouse_state_for_release.borrow_mut().pressed = false;
        });
        gl_area.add_controller(click);
    }

    if continuous_render {
        gl_area.add_tick_callback(|widget, _frame_clock| {
            widget.queue_render();
            glib::ControlFlow::Continue
        });
    }

    {
        let resources = resources.clone();
        let shader_path = shader_path.clone();
        let enable_sound = enable_sound;
        let sound_volume = sound_volume;
        gl_area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() {
                return;
            }

            let libgl = match unsafe { libloading::Library::new("libGL.so.1") } {
                Ok(lib) => lib,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: unable to open libGL.so.1: {err}");
                    return;
                }
            };

            let gl = unsafe {
                glow::Context::from_loader_function(|name| {
                    let symbol = format!("{name}\0");
                    libgl
                        .get::<*const core::ffi::c_void>(symbol.as_bytes())
                        .map(|s| *s)
                        .unwrap_or(std::ptr::null())
                })
            };

            let set = match discover_shadertoy_set(Path::new(&shader_path)) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("Shadertoy shader set error: {err}");
                    return;
                }
            };

            let common_src = set
                .common_path
                .as_ref()
                .and_then(|p| fs::read_to_string(p).ok())
                .map(|s| strip_shadertoy_version_lines(&s));

            let image_src = match fs::read_to_string(&set.image_path) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!(
                        "Shadertoy shader read failed ({}): {err}",
                        set.image_path.display()
                    );
                    return;
                }
            };
            let image_src = strip_shadertoy_version_lines(&image_src);
            let (image_program, image_uniforms, image_channel_kinds) = match unsafe {
                compile_shadertoy_program_auto(&gl, common_src.as_deref(), &image_src)
            } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!(
                        "Shadertoy image compile failed ({}): {err}",
                        set.image_path.display()
                    );
                    return;
                }
            };

            let mut buffers: [Option<ShadertoyProgram>; SHADER_CHANNELS] =
                std::array::from_fn(|_| None);
            for (idx, path) in set.buffer_paths.iter().enumerate() {
                let Some(path) = path else { continue };
                let src = match fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("Shadertoy buffer read failed ({}): {err}", path.display());
                        continue;
                    }
                };
                let src = strip_shadertoy_version_lines(&src);
                match unsafe { compile_shadertoy_program_auto(&gl, common_src.as_deref(), &src) } {
                    Ok((program, uniforms, channel_kinds)) => {
                        buffers[idx] = Some(ShadertoyProgram {
                            program,
                            uniforms,
                            channel_kinds,
                        });
                    }
                    Err(err) => eprintln!("Shadertoy buffer compile failed ({}): {err}", path.display()),
                };
            }

            let (blit_program, u_blit_tex) = match unsafe { compile_blit_program(&gl) } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: {err}");
                    unsafe {
                        gl.delete_program(image_program);
                    }
                    return;
                }
            };

            let vao = match unsafe { gl.create_vertex_array() } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create_vertex_array: {err}");
                    unsafe {
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let black_tex = match unsafe { create_black_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create textures: {err}");
                    unsafe {
                        gl.delete_vertex_array(vao);
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let noise_tex = match unsafe { create_noise_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create noise texture: {err}");
                    unsafe {
                        gl.delete_texture(black_tex);
                        gl.delete_vertex_array(vao);
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let black_cube_tex = match unsafe { create_black_cubemap_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create cubemap texture: {err}");
                    unsafe {
                        gl.delete_texture(noise_tex);
                        gl.delete_texture(black_tex);
                        gl.delete_vertex_array(vao);
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let black_3d_tex = match unsafe { create_black_3d_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create 3d texture: {err}");
                    unsafe {
                        gl.delete_texture(black_cube_tex);
                        gl.delete_texture(noise_tex);
                        gl.delete_texture(black_tex);
                        gl.delete_vertex_array(vao);
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let (fbo, fbo_tex) = match unsafe { create_fbo(&gl) } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create fbo: {err}");
                    unsafe {
                        gl.delete_texture(black_3d_tex);
                        gl.delete_texture(black_cube_tex);
                        gl.delete_texture(noise_tex);
                        gl.delete_texture(black_tex);
                        gl.delete_vertex_array(vao);
                        gl.delete_program(blit_program);
                        gl.delete_program(image_program);
                        for prog in buffers.iter().flatten() {
                            gl.delete_program(prog.program);
                        }
                    }
                    return;
                }
            };

            let external_channels = unsafe { load_external_channels(&gl, &set.asset_channels) };

            let sound = if enable_sound {
                set.sound_path.as_ref().and_then(|sound_path| {
                    init_sound_state(&gl, common_src.as_deref(), sound_path, sound_volume)
                        .map_err(|err| {
                            eprintln!("Shadertoy sound init failed ({}): {err}", sound_path.display());
                        })
                        .ok()
                })
            } else {
                None
            };

            *resources.borrow_mut() = Some(ShadertoyResources {
                _libgl: libgl,
                gl,
                image: ShadertoyProgram {
                    program: image_program,
                    uniforms: image_uniforms,
                    channel_kinds: image_channel_kinds,
                },
                buffers,
                external_channels,
                noise_tex,
                sound,
                blit_program,
                vao,
                black_tex,
                black_cube_tex,
                black_3d_tex,
                buffer_states: std::array::from_fn(|_| None),
                fbo,
                fbo_tex,
                fbo_size: (0, 0),
                u_blit_tex,
                start: Instant::now(),
                last_t: 0.0,
                frame: 0,
            });
        });
    }

    {
        let resources = resources.clone();
        gl_area.connect_unrealize(move |area| {
            if let Some(res) = resources.borrow_mut().take() {
                area.make_current();
                unsafe {
                    if let Some(mut sound) = res.sound {
                        sound.backend.kill_and_wait();
                        res.gl.delete_texture(sound.tex);
                        res.gl.delete_framebuffer(sound.fbo);
                        res.gl.delete_program(sound.program.program);
                    }
                    for ch in res.external_channels.iter().flatten() {
                        res.gl.delete_texture(ch.tex);
                    }
                    res.gl.delete_texture(res.noise_tex);
                    res.gl.delete_texture(res.black_3d_tex);
                    res.gl.delete_texture(res.black_cube_tex);
                    res.gl.delete_texture(res.black_tex);
                    for state in res.buffer_states.iter().flatten() {
                        res.gl.delete_texture(state.tex_prev);
                        res.gl.delete_texture(state.tex_next);
                        res.gl.delete_framebuffer(state.fbo);
                    }
                    res.gl.delete_texture(res.fbo_tex);
                    res.gl.delete_framebuffer(res.fbo);
                    res.gl.delete_vertex_array(res.vao);
                    res.gl.delete_program(res.blit_program);
                    res.gl.delete_program(res.image.program);
                    for prog in res.buffers.iter().flatten() {
                        res.gl.delete_program(prog.program);
                    }
                }
            }
        });
    }

    {
        let resources = resources.clone();
        let mouse_state = mouse_state.clone();
        gl_area.connect_render(move |area, _ctx| {
            let mut binding = resources.borrow_mut();
            let Some(res) = binding.as_mut() else {
                return glib::Propagation::Stop;
            };

            let scale = area.scale_factor().max(1) as f32;
            let target_w = (area.width() as f32 * scale).max(1.0);
            let target_h = (area.height() as f32 * scale).max(1.0);

            // Heavy Shadertoy shaders can be very expensive at 4K.
            // We render to a smaller offscreen texture and upscale.
            let quality = match density {
                PatternDensity::Low => 0.5,
                PatternDensity::Medium => 0.75,
                PatternDensity::High => 1.0,
            };
            let render_w = (target_w * quality).round().clamp(16.0, 8192.0) as i32;
            let render_h = (target_h * quality).round().clamp(16.0, 8192.0) as i32;

            let t = res.start.elapsed().as_secs_f32();
            let dt = (t - res.last_t).max(0.0);
            res.last_t = t;

            let state = *mouse_state.borrow();
            let (mx, my, mz, mw) = {
                let scale_pos = |(x, y): (f32, f32)| -> (f32, f32) {
                    let x = x * (render_w as f32 / target_w.max(1.0));
                    let y_from_top = y * (render_h as f32 / target_h.max(1.0));
                    // Shadertoy iMouse uses origin at bottom-left.
                    let y = (render_h as f32 - y_from_top).max(0.0);
                    (x, y)
                };
                let (mx, my) = state.pos.map(scale_pos).unwrap_or((0.0, 0.0));
                let (mut mz, mut mw) = state.last_press.map(scale_pos).unwrap_or((0.0, 0.0));
                if !state.pressed {
                    mz = -mz;
                    mw = -mw;
                }
                (mx, my, mz, mw)
            };

            let (year, month, day, seconds) = current_date_parts();

            unsafe {
                let prev_fbo_binding = res.gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
                let prev_fbo = fbo_from_gl_binding(prev_fbo_binding);

                if let Err(err) = ensure_fbo_size(&res.gl, res.fbo, res.fbo_tex, &mut res.fbo_size, render_w, render_h) {
                    eprintln!("Shadertoy: FBO resize failed: {err}");
                    return glib::Propagation::Stop;
                }

                res.gl.disable(glow::DEPTH_TEST);
                res.gl.disable(glow::STENCIL_TEST);
                res.gl.bind_vertex_array(Some(res.vao));

                // Ensure buffer states exist for enabled passes.
                for i in 0..SHADER_CHANNELS {
                    if res.buffers[i].is_some() && res.buffer_states[i].is_none() {
                        match create_buffer_state(&res.gl) {
                            Ok(state) => res.buffer_states[i] = Some(state),
                            Err(err) => eprintln!("Shadertoy: failed to init buffer {}: {err}", i),
                        }
                    }
                    if let Some(state) = res.buffer_states[i].as_mut() {
                        if let Err(err) = ensure_buffer_size(&res.gl, state, render_w, render_h) {
                            eprintln!("Shadertoy: buffer resize failed: {err}");
                        }
                    }
                }

                // Buffer passes (A..D). Each pass renders into tex_next, sampling previous frame (tex_prev)
                // and already-rendered passes this frame (tex_next for earlier buffers).
                for pass_idx in 0..SHADER_CHANNELS {
                    let Some(prog) = res.buffers[pass_idx].as_ref() else {
                        continue;
                    };
                    let channel_bindings = resolve_channel_bindings(res, pass_idx, true, t);
                    let Some(state) = res.buffer_states[pass_idx].as_mut() else {
                        continue;
                    };
                    bind_channels(
                        &res.gl,
                        &channel_bindings,
                        &prog.channel_kinds,
                        res.black_cube_tex,
                        res.black_3d_tex,
                    );

                    res.gl
                        .bind_framebuffer(glow::FRAMEBUFFER, Some(state.fbo));
                    res.gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(state.tex_next),
                        0,
                    );
                    res.gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
                    res.gl.viewport(0, 0, render_w, render_h);
                    res.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                    res.gl.clear(glow::COLOR_BUFFER_BIT);

                    res.gl.use_program(Some(prog.program));
                    set_channel_uniforms(
                        &res.gl,
                        &prog.uniforms,
                        &channel_bindings,
                        &prog.channel_kinds,
                        res.black_tex,
                    );
                    set_common_uniforms(
                        &res.gl,
                        &prog.uniforms,
                        t,
                        dt,
                        res.frame,
                        render_w as f32,
                        render_h as f32,
                        mx,
                        my,
                        mz,
                        mw,
                        year,
                        month,
                        day,
                        seconds,
                    );
                    res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }

                // Image pass: render into offscreen texture using current-frame buffer outputs.
                let image_channels = resolve_channel_bindings(res, SHADER_CHANNELS, false, t);
                bind_channels(
                    &res.gl,
                    &image_channels,
                    &res.image.channel_kinds,
                    res.black_cube_tex,
                    res.black_3d_tex,
                );

                res.gl
                    .bind_framebuffer(glow::FRAMEBUFFER, Some(res.fbo));
                res.gl.viewport(0, 0, render_w, render_h);
                res.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                res.gl.clear(glow::COLOR_BUFFER_BIT);

                res.gl.use_program(Some(res.image.program));
                set_channel_uniforms(
                    &res.gl,
                    &res.image.uniforms,
                    &image_channels,
                    &res.image.channel_kinds,
                    res.black_tex,
                );
                set_common_uniforms(
                    &res.gl,
                    &res.image.uniforms,
                    t,
                    dt,
                    res.frame,
                    render_w as f32,
                    render_h as f32,
                    mx,
                    my,
                    mz,
                    mw,
                    year,
                    month,
                    day,
                    seconds,
                );
                res.gl.draw_arrays(glow::TRIANGLES, 0, 3);

                if res.sound.is_some() {
                    // Borrow-split: `pump_sound` needs an immutable view of `res`, while sound needs
                    // a mutable borrow. Temporarily take the sound state out.
                    let mut sound = res.sound.take().expect("checked is_some");
                    if let Err(err) = pump_sound(&res.gl, res, &mut sound) {
                        eprintln!("Shadertoy sound: {err}");
                        cleanup_sound(&res.gl, &mut sound);
                    } else {
                        res.sound = Some(sound);
                    }
                }

                // Swap buffers after the image pass so next frame sees current output as previous frame.
                for i in 0..SHADER_CHANNELS {
                    if res.buffers[i].is_some() {
                        if let Some(state) = res.buffer_states[i].as_mut() {
                            std::mem::swap(&mut state.tex_prev, &mut state.tex_next);
                        }
                    }
                }

                // Upscale to screen.
                res.gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fbo);
                res.gl.viewport(0, 0, target_w as i32, target_h as i32);
                res.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                res.gl.clear(glow::COLOR_BUFFER_BIT);

                res.gl.use_program(Some(res.blit_program));
                res.gl.active_texture(glow::TEXTURE0);
                res.gl.bind_texture(glow::TEXTURE_2D, Some(res.fbo_tex));
                if let Some(loc) = &res.u_blit_tex {
                    res.gl.uniform_1_i32(Some(loc), 0);
                }
                res.gl.draw_arrays(glow::TRIANGLES, 0, 3);

                // Cleanup bindings.
                res.gl.bind_vertex_array(None);
                res.gl.use_program(None);
                for i in 0..SHADER_CHANNELS {
                    res.gl.active_texture(glow::TEXTURE0 + i as u32);
                    res.gl.bind_texture(glow::TEXTURE_2D, None);
                    res.gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
                    res.gl.bind_texture(glow::TEXTURE_3D, None);
                }
                res.gl.active_texture(glow::TEXTURE0);
                res.gl.bind_texture(glow::TEXTURE_2D, None);
                res.gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
                res.gl.bind_texture(glow::TEXTURE_3D, None);
            }

            res.frame = res.frame.wrapping_add(1);
            glib::Propagation::Stop
        });
    }

    gl_area
}

fn init_sound_state(
    gl: &glow::Context,
    common_src: Option<&str>,
    sound_path: &Path,
    volume: f32,
) -> Result<SoundState, String> {
    if !sound_path.is_file() {
        return Err("Sound.glsl not found".to_string());
    }
    let sound_src = fs::read_to_string(sound_path)
        .map_err(|e| format!("read {}: {e}", sound_path.display()))?;
    let sound_src = strip_shadertoy_version_lines(&sound_src);

    let (program, uniforms, channel_kinds) =
        unsafe { compile_shadertoy_sound_program_auto(gl, common_src, &sound_src) }?;
    let u_sample_rate = unsafe { gl.get_uniform_location(program, "iSampleRate") };
    let u_block_offset = unsafe { gl.get_uniform_location(program, "iBlockOffset") };

    let (fbo, tex) = match unsafe { create_sound_fbo(gl, SOUND_BLOCK_SAMPLES, 1) } {
        Ok(v) => v,
        Err(err) => {
            unsafe { gl.delete_program(program) };
            return Err(err);
        }
    };

    let backend = match spawn_audio_backend() {
        Ok(b) => b,
        Err(err) => {
            unsafe {
                gl.delete_texture(tex);
                gl.delete_framebuffer(fbo);
                gl.delete_program(program);
            }
            return Err(err);
        }
    };

    Ok(SoundState {
        program: SoundProgram {
            program,
            uniforms,
            channel_kinds,
            u_sample_rate,
            u_block_offset,
        },
        fbo,
        tex,
        pixels: vec![0.0; (SOUND_BLOCK_SAMPLES as usize) * 4],
        bytes: Vec::with_capacity((SOUND_BLOCK_SAMPLES as usize) * 2 * 4),
        next_sample: 0,
        block_index: 0,
        volume,
        backend,
    })
}

fn spawn_audio_backend() -> Result<SoundBackend, String> {
    // Prefer PipeWire (pw-cat), fallback to PulseAudio (paplay).
    let pw_err = match spawn_pw_cat() {
        Ok(pw) => return Ok(SoundBackend::PwCat(pw)),
        Err(err) => err,
    };
    match spawn_paplay() {
        Ok(pa) => Ok(SoundBackend::PaPlay(pa)),
        Err(pa_err) => Err(format!(
            "{pw_err}; fallback {pa_err} (no PipeWire/PulseAudio session?)"
        )),
    }
}

fn spawn_pw_cat() -> Result<Child, String> {
    let mut pw = Command::new("pw-cat")
        .args([
            "--playback",
            "--raw",
            "--format",
            "f32",
            "--rate",
            &format!("{}", SOUND_SAMPLE_RATE as u32),
            "--channels",
            "2",
            "--latency",
            "100ms",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn pw-cat: {e}"))?;

    if pw.stdin.is_none() {
        let _ = pw.kill();
        let _ = pw.wait();
        return Err("pw-cat stdin is not available".to_string());
    }
    // If it exits immediately, treat as unavailable (e.g. no PipeWire session).
    if let Ok(Some(status)) = pw.try_wait() {
        return Err(format!("pw-cat exited early: {status}"));
    }
    Ok(pw)
}

fn spawn_paplay() -> Result<Child, String> {
    let mut pa = Command::new("paplay")
        .args([
            "--playback",
            "--raw",
            "--format=float32le",
            "--rate",
            &format!("{}", SOUND_SAMPLE_RATE as u32),
            "--channels=2",
            "--client-name=vesper",
            "--stream-name=Vesper Shadertoy",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn paplay: {e}"))?;
    if pa.stdin.is_none() {
        let _ = pa.kill();
        let _ = pa.wait();
        return Err("paplay stdin is not available".to_string());
    }
    // If it exits immediately, treat as unavailable.
    if let Ok(Some(status)) = pa.try_wait() {
        return Err(format!("paplay exited early: {status}"));
    }
    Ok(pa)
}

fn cleanup_sound(gl: &glow::Context, sound: &mut SoundState) {
    sound.backend.kill_and_wait();
    unsafe {
        gl.delete_texture(sound.tex);
        gl.delete_framebuffer(sound.fbo);
        gl.delete_program(sound.program.program);
    }
}

unsafe fn create_sound_fbo(
    gl: &glow::Context,
    w: i32,
    h: i32,
) -> Result<(glow::NativeFramebuffer, glow::NativeTexture), String> {
    let (fbo, tex) = create_fbo(gl)?;

    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA32F as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::FLOAT,
        None,
    );
    gl.bind_texture(glow::TEXTURE_2D, None);

    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );
    gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_texture(tex);
        gl.delete_framebuffer(fbo);
        return Err(format!("sound framebuffer incomplete: 0x{status:x}"));
    }

    Ok((fbo, tex))
}

fn pump_sound(
    gl: &glow::Context,
    res: &ShadertoyResources,
    sound: &mut SoundState,
) -> Result<(), String> {
    // Best-effort: generate audio slightly ahead of real time.
    let t = res.start.elapsed().as_secs_f32();
    let target_sample = (t * SOUND_SAMPLE_RATE) as i32;
    let ahead = (SOUND_AHEAD_SECONDS * SOUND_SAMPLE_RATE) as i32;
    let max_sample = target_sample.saturating_add(ahead);
    if sound.next_sample >= max_sample {
        return Ok(());
    }

    ensure_sound_backend(sound)?;

    unsafe {
        let prev_fbo_binding = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let prev_fbo = fbo_from_gl_binding(prev_fbo_binding);

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(sound.fbo));
        gl.viewport(0, 0, SOUND_BLOCK_SAMPLES, 1);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.use_program(Some(sound.program.program));

        if let Some(loc) = &sound.program.u_sample_rate {
            gl.uniform_1_f32(Some(loc), SOUND_SAMPLE_RATE);
        }
        if let Some(loc) = &sound.program.u_block_offset {
            gl.uniform_1_i32(Some(loc), sound.next_sample);
        }

        let block_time = sound.next_sample as f32 / SOUND_SAMPLE_RATE;
        let dt = SOUND_BLOCK_SAMPLES as f32 / SOUND_SAMPLE_RATE;
        let channels = resolve_channel_bindings(res, SHADER_CHANNELS, false, block_time);
        bind_channels(
            gl,
            &channels,
            &sound.program.channel_kinds,
            res.black_cube_tex,
            res.black_3d_tex,
        );
        set_channel_uniforms(
            gl,
            &sound.program.uniforms,
            &channels,
            &sound.program.channel_kinds,
            res.black_tex,
        );
        set_common_uniforms(
            gl,
            &sound.program.uniforms,
            block_time,
            dt,
            sound.block_index,
            SOUND_BLOCK_SAMPLES as f32,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        gl.draw_arrays(glow::TRIANGLES, 0, 3);

        gl.read_pixels(
            0,
            0,
            SOUND_BLOCK_SAMPLES,
            1,
            glow::RGBA,
            glow::FLOAT,
            glow::PixelPackData::Slice(bytemuck::cast_slice_mut(&mut sound.pixels)),
        );

        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_fbo);
    }

    sound.bytes.clear();
    sound.bytes.reserve((SOUND_BLOCK_SAMPLES as usize) * 2 * 4);
    for i in 0..SOUND_BLOCK_SAMPLES as usize {
        let l = (sound.pixels[i * 4] * sound.volume).clamp(-1.0, 1.0);
        let r = (sound.pixels[i * 4 + 1] * sound.volume).clamp(-1.0, 1.0);
        sound.bytes.extend_from_slice(&l.to_le_bytes());
        sound.bytes.extend_from_slice(&r.to_le_bytes());
    }
    if let Err(err) = write_sound(sound) {
        // pw-cat can be present but non-functional in some environments (e.g. no PipeWire session).
        // Try a best-effort runtime fallback to paplay once.
        if sound.backend.is_pw_cat() {
            sound.backend.kill_and_wait();
            let pa = spawn_paplay()
                .map_err(|e| format!("{err}; fallback {e} (no PipeWire/PulseAudio session?)"))?;
            sound.backend = SoundBackend::PaPlay(pa);
            write_sound(sound)
                .map_err(|e| format!("{err}; fallback {e} (no PipeWire/PulseAudio session?)"))?;
        } else {
            return Err(err);
        }
    }

    sound.next_sample = sound.next_sample.saturating_add(SOUND_BLOCK_SAMPLES);
    sound.block_index = sound.block_index.wrapping_add(1);
    Ok(())
}

fn ensure_sound_backend(sound: &mut SoundState) -> Result<(), String> {
    if let Some(status) = sound
        .backend
        .try_wait()
        .map_err(|e| format!("{}: {e}", sound.backend.kind_name()))?
    {
        // If pw-cat died, try switching to paplay once.
        if sound.backend.is_pw_cat() {
            sound.backend.kill_and_wait();
            match spawn_paplay() {
                Ok(pa) => {
                    sound.backend = SoundBackend::PaPlay(pa);
                    return Ok(());
                }
                Err(err) => {
                    return Err(format!(
                        "pw-cat exited: {status}; fallback {err} (no PipeWire/PulseAudio session?)"
                    ));
                }
            }
        }
        return Err(format!("{} exited: {status}", sound.backend.kind_name()));
    }
    let name = sound.backend.kind_name();
    if sound.backend.stdin_mut().is_none() {
        return Err(format!("{name} stdin closed"));
    }
    Ok(())
}

fn write_sound(sound: &mut SoundState) -> Result<(), String> {
    let name = sound.backend.kind_name();
    let stdin = sound
        .backend
        .stdin_mut()
        .ok_or_else(|| format!("{name} stdin closed"))?;
    let bytes = sound.bytes.as_slice();
    stdin
        .write_all(bytes)
        .map_err(|e| format!("{name} write: {e}"))
}

fn wrap_shadertoy_fragment(
    common_src: Option<&str>,
    user_src: &str,
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
) -> String {
    // Expect Shadertoy-style snippet with `mainImage(out vec4, in vec2)`.
    // We provide minimal Shadertoy uniforms and 4 dummy channels (black textures).
    let mut out = String::new();
    out.push_str("#version 330 core\n");
    out.push_str("#define texture2D texture\n");
    out.push_str("#define textureCube texture\n");
    out.push_str("out vec4 fragColor;\n");
    out.push_str("uniform vec3 iResolution;\n");
    out.push_str("uniform float iTime;\n");
    out.push_str("uniform float iTimeDelta;\n");
    out.push_str("uniform float iFrameRate;\n");
    out.push_str("uniform int iFrame;\n");
    out.push_str("uniform vec4 iMouse;\n");
    out.push_str("uniform vec4 iDate;\n");
    out.push_str("uniform float iChannelTime[4];\n");
    out.push_str("uniform vec3 iChannelResolution[4];\n");
    for i in 0..SHADER_CHANNELS {
        out.push_str(&format!("uniform sampler2D iChannel{i}_2d;\n"));
        out.push_str(&format!("uniform samplerCube iChannel{i}_cube;\n"));
        out.push_str(&format!("uniform sampler3D iChannel{i}_3d;\n"));
    }
    for i in 0..SHADER_CHANNELS {
        let alias = match channel_kinds[i] {
            SamplerKind::Tex2D => format!("iChannel{i}_2d"),
            SamplerKind::Cube => format!("iChannel{i}_cube"),
            SamplerKind::Tex3D => format!("iChannel{i}_3d"),
        };
        out.push_str(&format!("#define iChannel{i} {alias}\n"));
    }
    out.push_str("\n");
    if let Some(common_src) = common_src {
        let common_src = common_src.trim();
        if !common_src.is_empty() {
            out.push_str(common_src);
            out.push_str("\n\n");
        }
    }
    out.push_str(user_src);
    out.push_str("\n\n");
    // Some shader packs redefine `mainImage` as a macro (e.g. for manual anti-aliasing). That
    // breaks our `mainImage(...)` call below. Undefine it after user code so the function can be
    // called normally.
    out.push_str("#ifdef mainImage\n#undef mainImage\n#endif\n\n");
    out.push_str("void main() {\n");
    out.push_str("    mainImage(fragColor, gl_FragCoord.xy);\n");
    out.push_str("}\n");
    out
}

fn wrap_shadertoy_sound_fragment(
    common_src: Option<&str>,
    user_src: &str,
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
) -> String {
    // Shadertoy "Sound" pass is still a fragment shader. We render a 1D block into a float texture
    // and stream the (L,R) samples from fragColor.rg.
    let mut out = String::new();
    out.push_str("#version 330 core\n");
    out.push_str("#define texture2D texture\n");
    out.push_str("#define textureCube texture\n");
    out.push_str("out vec4 fragColor;\n");
    out.push_str("uniform vec3 iResolution;\n");
    out.push_str("uniform float iTime;\n");
    out.push_str("uniform float iTimeDelta;\n");
    out.push_str("uniform float iFrameRate;\n");
    out.push_str("uniform int iFrame;\n");
    out.push_str("uniform vec4 iMouse;\n");
    out.push_str("uniform vec4 iDate;\n");
    out.push_str("uniform float iChannelTime[4];\n");
    out.push_str("uniform vec3 iChannelResolution[4];\n");
    out.push_str("uniform float iSampleRate;\n");
    out.push_str("uniform int iBlockOffset;\n");
    for i in 0..SHADER_CHANNELS {
        out.push_str(&format!("uniform sampler2D iChannel{i}_2d;\n"));
        out.push_str(&format!("uniform samplerCube iChannel{i}_cube;\n"));
        out.push_str(&format!("uniform sampler3D iChannel{i}_3d;\n"));
    }
    for i in 0..SHADER_CHANNELS {
        let alias = match channel_kinds[i] {
            SamplerKind::Tex2D => format!("iChannel{i}_2d"),
            SamplerKind::Cube => format!("iChannel{i}_cube"),
            SamplerKind::Tex3D => format!("iChannel{i}_3d"),
        };
        out.push_str(&format!("#define iChannel{i} {alias}\n"));
    }
    out.push_str("\n");
    if let Some(common_src) = common_src {
        let common_src = common_src.trim();
        if !common_src.is_empty() {
            out.push_str(common_src);
            out.push_str("\n\n");
        }
    }
    out.push_str(user_src);
    out.push_str("\n\n");
    out.push_str("#ifdef mainSound\n#undef mainSound\n#endif\n\n");
    out.push_str("void main() {\n");
    out.push_str("    int x = int(gl_FragCoord.x) - 1;\n");
    out.push_str("    int y = int(gl_FragCoord.y) - 1;\n");
    out.push_str("    int w = int(iResolution.x);\n");
    out.push_str("    int samp = iBlockOffset + x + y * w;\n");
    out.push_str("    float time = float(samp) / iSampleRate;\n");
    out.push_str("    vec2 s = mainSound(samp, time);\n");
    out.push_str("    fragColor = vec4(s, 0.0, 1.0);\n");
    out.push_str("}\n");
    out
}

fn strip_shadertoy_version_lines(src: &str) -> String {
    // Backward-compatible alias for older callsites.
    sanitize_shadertoy_source(src)
}

fn sanitize_shadertoy_source(src: &str) -> String {
    // Shadertoy snippets are often copy-pasted from WebGL contexts. We provide a few best-effort
    // sanitizers so more shaders compile in desktop GLSL 330 core.
    //
    // - Strip `#version` lines (we provide our own).
    // - Strip `precision ...` lines (not valid in desktop GLSL).
    // - Fix a common "comment toggle" artifact found in some shader collections where a line
    //   starts with `/ */` (which prematurely terminates a `/* ... */` block). We make it `* /`
    //   so it stays inside the comment.
    let mut out = String::new();
    let mut first = true;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#version") {
            continue;
        }
        if trimmed.starts_with("precision ") {
            continue;
        }

        let fixed = sanitize_comment_toggle_line(line);
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&fixed);
    }
    out
}

fn sanitize_comment_toggle_line(line: &str) -> String {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('/') {
        return line.to_string();
    }
    let after_slash = trimmed[1..].trim_start();
    if !after_slash.starts_with("*/") {
        return line.to_string();
    }
    // Replace the first `*/` in the original line with `* /` (insert a space), preventing an
    // accidental block comment terminator.
    if let Some(pos) = line.find("*/") {
        let mut out = String::with_capacity(line.len() + 1);
        out.push_str(&line[..pos]);
        out.push_str("* /");
        out.push_str(&line[pos + 2..]);
        return out;
    }
    line.to_string()
}

#[derive(Clone, Copy)]
struct Uniforms {
    u_time: Option<glow::NativeUniformLocation>,
    u_time_delta: Option<glow::NativeUniformLocation>,
    u_frame_rate: Option<glow::NativeUniformLocation>,
    u_frame: Option<glow::NativeUniformLocation>,
    u_resolution: Option<glow::NativeUniformLocation>,
    u_mouse: Option<glow::NativeUniformLocation>,
    u_date: Option<glow::NativeUniformLocation>,
    u_channel_time: [Option<glow::NativeUniformLocation>; SHADER_CHANNELS],
    u_channel_resolution: [Option<glow::NativeUniformLocation>; SHADER_CHANNELS],
}

unsafe fn compile_shadertoy_program(
    gl: &glow::Context,
    fragment_src: &str,
) -> Result<(glow::NativeProgram, Uniforms), String> {
    let vs_src = fullscreen_triangle_vs();

    let program = gl
        .create_program()
        .map_err(|e| format!("create_program: {e}"))?;

    let vs = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| format!("create_shader(vs): {e}"))?;
    gl.shader_source(vs, vs_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        let log = gl.get_shader_info_log(vs);
        gl.delete_shader(vs);
        gl.delete_program(program);
        return Err(format!("vertex shader compile failed: {log}"));
    }

    let fs = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| format!("create_shader(fs): {e}"))?;
    gl.shader_source(fs, fragment_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        let log = gl.get_shader_info_log(fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        gl.delete_program(program);
        return Err(format!("fragment shader compile failed: {log}"));
    }

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("program link failed: {log}"));
    }

    gl.use_program(Some(program));
    // Bind iChannel0..3 to texture units 0..3 (all variants; wrapper aliases the active one).
    for (i, unit) in (0..SHADER_CHANNELS).enumerate() {
        for name in [
            format!("iChannel{i}_2d"),
            format!("iChannel{i}_cube"),
            format!("iChannel{i}_3d"),
        ] {
            if let Some(loc) = gl.get_uniform_location(program, &name) {
                gl.uniform_1_i32(Some(&loc), unit as i32);
            }
        }
    }
    gl.use_program(None);

    Ok((
        program,
        Uniforms {
            u_time: gl.get_uniform_location(program, "iTime"),
            u_time_delta: gl.get_uniform_location(program, "iTimeDelta"),
            u_frame_rate: gl.get_uniform_location(program, "iFrameRate"),
            u_frame: gl.get_uniform_location(program, "iFrame"),
            u_resolution: gl.get_uniform_location(program, "iResolution"),
            u_mouse: gl.get_uniform_location(program, "iMouse"),
            u_date: gl.get_uniform_location(program, "iDate"),
            u_channel_time: std::array::from_fn(|i| {
                let name = format!("iChannelTime[{i}]");
                gl.get_uniform_location(program, &name)
            }),
            u_channel_resolution: std::array::from_fn(|i| {
                let name = format!("iChannelResolution[{i}]");
                gl.get_uniform_location(program, &name)
            }),
        },
    ))
}

unsafe fn compile_shadertoy_program_auto(
    gl: &glow::Context,
    common_src: Option<&str>,
    user_src: &str,
) -> Result<(glow::NativeProgram, Uniforms, [SamplerKind; SHADER_CHANNELS]), String> {
    let mut kinds = [SamplerKind::Tex2D; SHADER_CHANNELS];
    let mut last_err: Option<String> = None;

    for _ in 0..5 {
        let wrapped = wrap_shadertoy_fragment(common_src, user_src, &kinds);
        match compile_shadertoy_program(gl, &wrapped) {
            Ok((program, uniforms)) => return Ok((program, uniforms, kinds)),
            Err(err) => {
                let log = err
                    .strip_prefix("fragment shader compile failed: ")
                    .unwrap_or(&err);
                let changed = infer_channel_kinds_from_compile_log(&wrapped, log, &mut kinds);
                last_err = Some(err);
                if !changed {
                    break;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "unknown shader compile error".to_string()))
}

unsafe fn compile_shadertoy_sound_program_auto(
    gl: &glow::Context,
    common_src: Option<&str>,
    user_src: &str,
) -> Result<(glow::NativeProgram, Uniforms, [SamplerKind; SHADER_CHANNELS]), String> {
    let mut kinds = [SamplerKind::Tex2D; SHADER_CHANNELS];
    let mut last_err: Option<String> = None;

    for _ in 0..5 {
        let wrapped = wrap_shadertoy_sound_fragment(common_src, user_src, &kinds);
        match compile_shadertoy_program(gl, &wrapped) {
            Ok((program, uniforms)) => return Ok((program, uniforms, kinds)),
            Err(err) => {
                let log = err
                    .strip_prefix("fragment shader compile failed: ")
                    .unwrap_or(&err);
                let changed = infer_channel_kinds_from_compile_log(&wrapped, log, &mut kinds);
                last_err = Some(err);
                if !changed {
                    break;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "unknown shader compile error".to_string()))
}

fn infer_channel_kinds_from_compile_log(
    wrapped_src: &str,
    compile_log: &str,
    kinds: &mut [SamplerKind; SHADER_CHANNELS],
) -> bool {
    // Common issue: some Shadertoy shaders sample iChannelN with a coordinate type that doesn't
    // match the inferred sampler type. We use compile logs to adjust iChannelN sampler kinds.
    let src_lines: Vec<&str> = wrapped_src.lines().collect();
    let mut changed = false;

    for log_line in compile_log.lines() {
        let desired = if log_line.contains("call to `texture(sampler2D, vec3")
            || log_line.contains("call to `textureLod(sampler2D, vec3")
            || log_line.contains("call to `textureGrad(sampler2D, vec3")
        {
            Some(SamplerKind::Cube)
        } else if log_line.contains("call to `texture(samplerCube, vec2")
            || log_line.contains("call to `textureLod(samplerCube, vec2")
            || log_line.contains("call to `textureGrad(samplerCube, vec2")
        {
            Some(SamplerKind::Tex2D)
        } else {
            None
        };
        let Some(desired) = desired else { continue };

        // Example: `0:1056(13): error: ...`
        let Some((_, after_prefix)) = log_line.split_once("0:") else {
            continue;
        };
        let line_no_str: String = after_prefix
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        let Ok(line_no) = line_no_str.parse::<usize>() else {
            continue;
        };
        let Some(src_line) = src_lines.get(line_no.saturating_sub(1)).copied() else {
            continue;
        };

        for ch in 0..SHADER_CHANNELS {
            if src_line.contains(&format!("iChannel{ch}")) {
                if kinds[ch] != desired {
                    kinds[ch] = desired;
                    changed = true;
                }
            }
        }
    }

    changed
}

unsafe fn compile_blit_program(
    gl: &glow::Context,
) -> Result<(glow::NativeProgram, Option<glow::NativeUniformLocation>), String> {
    let vs_src = fullscreen_triangle_vs();
    let fs_src = r#"#version 330 core
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D u_tex;
void main() {
    fragColor = texture(u_tex, v_uv);
}
"#;

    let program = gl
        .create_program()
        .map_err(|e| format!("create_program: {e}"))?;

    let vs = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| format!("create_shader(vs): {e}"))?;
    gl.shader_source(vs, vs_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        let log = gl.get_shader_info_log(vs);
        gl.delete_shader(vs);
        gl.delete_program(program);
        return Err(format!("vertex shader compile failed: {log}"));
    }

    let fs = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| format!("create_shader(fs): {e}"))?;
    gl.shader_source(fs, fs_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        let log = gl.get_shader_info_log(fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        gl.delete_program(program);
        return Err(format!("fragment shader compile failed: {log}"));
    }

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("program link failed: {log}"));
    }

    let u_tex = gl.get_uniform_location(program, "u_tex");
    Ok((program, u_tex))
}

fn fullscreen_triangle_vs() -> &'static str {
    r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 pos;
    if (gl_VertexID == 0) pos = vec2(-1.0, -1.0);
    else if (gl_VertexID == 1) pos = vec2(3.0, -1.0);
    else pos = vec2(-1.0, 3.0);
    gl_Position = vec4(pos, 0.0, 1.0);
    v_uv = 0.5 * (pos + 1.0);
}
"#
}

unsafe fn create_fbo(
    gl: &glow::Context,
) -> Result<(glow::NativeFramebuffer, glow::NativeTexture), String> {
    let fbo = gl
        .create_framebuffer()
        .map_err(|e| format!("create_framebuffer: {e}"))?;
    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok((fbo, tex))
}

unsafe fn ensure_fbo_size(
    gl: &glow::Context,
    fbo: glow::NativeFramebuffer,
    tex: glow::NativeTexture,
    size: &mut (i32, i32),
    w: i32,
    h: i32,
) -> Result<(), String> {
    if *size == (w, h) {
        return Ok(());
    }
    *size = (w, h);

    let prev = fbo_from_gl_binding(gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING));

    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        None,
    );
    gl.bind_texture(glow::TEXTURE_2D, None);

    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );
    gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    if status != glow::FRAMEBUFFER_COMPLETE {
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
        return Err(format!("framebuffer incomplete: 0x{status:x}"));
    }
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev);
    Ok(())
}

fn fbo_from_gl_binding(binding: i32) -> Option<glow::NativeFramebuffer> {
    if binding <= 0 {
        None
    } else {
        std::num::NonZeroU32::new(binding as u32).map(glow::NativeFramebuffer)
    }
}

unsafe fn create_black_texture(gl: &glow::Context) -> Result<glow::NativeTexture, String> {
    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;
    init_rgba8_texture(gl, tex);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    let black: [u8; 4] = [0, 0, 0, 255];
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        1,
        1,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        Some(&black),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(tex)
}

unsafe fn create_texture_from_image(
    gl: &glow::Context,
    path: &Path,
) -> Result<(glow::NativeTexture, (i32, i32)), String> {
    let img = image::open(path).map_err(|e| format!("image::open({}): {e}", path.display()))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;
    init_rgba8_texture(gl, tex);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w as i32,
        h as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        Some(rgba.as_raw()),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok((tex, (w as i32, h as i32)))
}

unsafe fn load_external_channels(
    gl: &glow::Context,
    paths: &[Option<PathBuf>; SHADER_CHANNELS],
) -> [Option<ExternalChannel>; SHADER_CHANNELS] {
    std::array::from_fn(|i| {
        let Some(path) = paths[i].as_ref() else {
            return None;
        };
        match create_texture_from_image(gl, path) {
            Ok((tex, size)) => Some(ExternalChannel { tex, size }),
            Err(err) => {
                eprintln!(
                    "Shadertoy: failed to load iChannel{i} asset ({}): {err}",
                    path.display()
                );
                None
            }
        }
    })
}

unsafe fn create_black_cubemap_texture(gl: &glow::Context) -> Result<glow::NativeTexture, String> {
    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;

    gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_CUBE_MAP,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_CUBE_MAP,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_CUBE_MAP,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_CUBE_MAP,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_CUBE_MAP,
        glow::TEXTURE_WRAP_R,
        glow::CLAMP_TO_EDGE as i32,
    );

    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    let black: [u8; 4] = [0, 0, 0, 255];
    for face in 0..6 {
        gl.tex_image_2d(
            glow::TEXTURE_CUBE_MAP_POSITIVE_X + face,
            0,
            glow::RGBA8 as i32,
            1,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            Some(&black),
        );
    }
    gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
    Ok(tex)
}

unsafe fn create_noise_texture(gl: &glow::Context) -> Result<glow::NativeTexture, String> {
    const W: i32 = 256;
    const H: i32 = 256;

    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;

    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::REPEAT as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::REPEAT as i32,
    );

    let mut seed: u32 = 0x1234_5678;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };

    let mut data: Vec<u8> = vec![0; (W as usize) * (H as usize) * 4];
    for px in data.chunks_exact_mut(4) {
        let v = (rng() & 0xff) as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
        px[3] = 255;
    }

    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        W,
        H,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        Some(&data),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(tex)
}

unsafe fn create_black_3d_texture(gl: &glow::Context) -> Result<glow::NativeTexture, String> {
    let tex = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;

    gl.bind_texture(glow::TEXTURE_3D, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_3D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_3D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_3D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_3D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_3D,
        glow::TEXTURE_WRAP_R,
        glow::CLAMP_TO_EDGE as i32,
    );

    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    let black: [u8; 4] = [0, 0, 0, 255];
    gl.tex_image_3d(
        glow::TEXTURE_3D,
        0,
        glow::RGBA8 as i32,
        1,
        1,
        1,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        Some(&black),
    );
    gl.bind_texture(glow::TEXTURE_3D, None);
    Ok(tex)
}

unsafe fn init_rgba8_texture(gl: &glow::Context, tex: glow::NativeTexture) {
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
}

unsafe fn create_buffer_state(gl: &glow::Context) -> Result<BufferState, String> {
    let fbo = gl
        .create_framebuffer()
        .map_err(|e| format!("create_framebuffer: {e}"))?;
    let tex_prev = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;
    let tex_next = gl
        .create_texture()
        .map_err(|e| format!("create_texture: {e}"))?;
    init_rgba8_texture(gl, tex_prev);
    init_rgba8_texture(gl, tex_next);
    Ok(BufferState {
        fbo,
        tex_prev,
        tex_next,
        size: (0, 0),
    })
}

unsafe fn ensure_buffer_size(
    gl: &glow::Context,
    state: &mut BufferState,
    w: i32,
    h: i32,
) -> Result<(), String> {
    if state.size == (w, h) {
        return Ok(());
    }
    state.size = (w, h);
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    for tex in [state.tex_prev, state.tex_next] {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            None,
        );
    }
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(())
}

unsafe fn bind_channels(
    gl: &glow::Context,
    bindings: &[ChannelBinding; SHADER_CHANNELS],
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
    black_cube_tex: glow::NativeTexture,
    black_3d_tex: glow::NativeTexture,
) {
    for (i, binding) in bindings.iter().enumerate() {
        gl.active_texture(glow::TEXTURE0 + i as u32);
        match channel_kinds[i] {
            SamplerKind::Tex2D => {
                gl.bind_texture(glow::TEXTURE_2D, Some(binding.tex));
                gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
                gl.bind_texture(glow::TEXTURE_3D, None);
            }
            SamplerKind::Cube => {
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(black_cube_tex));
                gl.bind_texture(glow::TEXTURE_3D, None);
            }
            SamplerKind::Tex3D => {
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
                gl.bind_texture(glow::TEXTURE_3D, Some(black_3d_tex));
            }
        }
    }
    gl.active_texture(glow::TEXTURE0);
}

#[derive(Clone, Copy)]
struct ChannelBinding {
    tex: glow::NativeTexture,
    size: (i32, i32),
    time: f32,
}

fn resolve_channel_bindings(
    res: &ShadertoyResources,
    pass_idx: usize,
    buffer_pass: bool,
    t: f32,
) -> [ChannelBinding; SHADER_CHANNELS] {
    std::array::from_fn(|ch| {
        // BufferA..D are exposed to the Image pass as iChannel0..3. For buffer passes, only buffers
        // from earlier passes are exposed to avoid accidental self-feedback loops.
        if res.buffers[ch].is_some() && (!buffer_pass || ch < pass_idx) {
            let Some(state) = res.buffer_states[ch].as_ref() else {
                return ChannelBinding {
                    tex: res.black_tex,
                    size: (1, 1),
                    time: 0.0,
                };
            };
            let tex = state.tex_next;
            let size = if state.size.0 > 0 && state.size.1 > 0 {
                state.size
            } else {
                (1, 1)
            };
            return ChannelBinding { tex, size, time: t };
        }

        if let Some(ext) = res.external_channels[ch].as_ref() {
            return ChannelBinding {
                tex: ext.tex,
                size: ext.size,
                time: 0.0,
            };
        }

        // Best-effort: common Shadertoy buffer passes expect iChannel0=Noise when no assets are
        // configured (e.g. "Elevated" by iq). Provide a built-in 256x256 noise texture.
        if buffer_pass && ch == 0 {
            return ChannelBinding {
                tex: res.noise_tex,
                size: (256, 256),
                time: 0.0,
            };
        }

        ChannelBinding {
            tex: res.black_tex,
            size: (1, 1),
            time: 0.0,
        }
    })
}

unsafe fn set_common_uniforms(
    gl: &glow::Context,
    uniforms: &Uniforms,
    t: f32,
    dt: f32,
    frame: i32,
    w: f32,
    h: f32,
    mx: f32,
    my: f32,
    mz: f32,
    mw: f32,
    year: f32,
    month: f32,
    day: f32,
    seconds: f32,
) {
    if let Some(loc) = &uniforms.u_time {
        gl.uniform_1_f32(Some(loc), t);
    }
    if let Some(loc) = &uniforms.u_time_delta {
        gl.uniform_1_f32(Some(loc), dt);
    }
    if let Some(loc) = &uniforms.u_frame_rate {
        let fr = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        gl.uniform_1_f32(Some(loc), fr);
    }
    if let Some(loc) = &uniforms.u_frame {
        gl.uniform_1_i32(Some(loc), frame);
    }
    if let Some(loc) = &uniforms.u_resolution {
        gl.uniform_3_f32(Some(loc), w, h, 1.0);
    }
    if let Some(loc) = &uniforms.u_mouse {
        gl.uniform_4_f32(Some(loc), mx, my, mz, mw);
    }
    if let Some(loc) = &uniforms.u_date {
        gl.uniform_4_f32(Some(loc), year, month, day, seconds);
    }
}

unsafe fn set_channel_uniforms(
    gl: &glow::Context,
    uniforms: &Uniforms,
    channels: &[ChannelBinding; SHADER_CHANNELS],
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
    black_tex: glow::NativeTexture,
) {
    for i in 0..SHADER_CHANNELS {
        if channel_kinds[i] != SamplerKind::Tex2D {
            if let Some(loc) = &uniforms.u_channel_resolution[i] {
                gl.uniform_3_f32(Some(loc), 1.0, 1.0, 1.0);
            }
            if let Some(loc) = &uniforms.u_channel_time[i] {
                gl.uniform_1_f32(Some(loc), 0.0);
            }
            continue;
        }

        let binding = channels[i];
        let is_black = binding.tex == black_tex;
        let (cw, ch) = if is_black {
            (1.0, 1.0)
        } else {
            (binding.size.0.max(1) as f32, binding.size.1.max(1) as f32)
        };
        if let Some(loc) = &uniforms.u_channel_resolution[i] {
            gl.uniform_3_f32(Some(loc), cw, ch, 1.0);
        }
        if let Some(loc) = &uniforms.u_channel_time[i] {
            gl.uniform_1_f32(Some(loc), if is_black { 0.0 } else { binding.time });
        }
    }
}

fn current_date_parts() -> (f32, f32, f32, f32) {
    // iDate: (year, month, day, seconds)
    // Keep it simple and dependency-free; best-effort UTC-like values.
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = (since_epoch.as_secs() / 86_400) as i64;
    let seconds = (since_epoch.as_secs() % 86_400) as f32;

    // Convert days since epoch to a rough date (UTC). This is a small helper,
    // not intended to be perfect for all historical dates.
    let (year, month, day) = days_to_ymd(days);
    (year as f32, month as f32, day as f32, seconds)
}

fn days_to_ymd(days_since_epoch: i64) -> (i32, i32, i32) {
    // 1970-01-01 is day 0. Algorithm: Howard Hinnant's civil_from_days.
    let z = days_since_epoch + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as i32, d as i32)
}

struct ShadertoySet {
    image_path: PathBuf,
    common_path: Option<PathBuf>,
    buffer_paths: [Option<PathBuf>; SHADER_CHANNELS],
    sound_path: Option<PathBuf>,
    asset_channels: [Option<PathBuf>; SHADER_CHANNELS],
}

fn discover_shadertoy_set(path: &Path) -> Result<ShadertoySet, String> {
    let (base_dir, selected_file): (PathBuf, Option<PathBuf>) = if path.is_dir() {
        (path.to_path_buf(), None)
    } else if path.is_file() {
        (
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
            Some(path.to_path_buf()),
        )
    } else {
        return Err(format!("path not found: {}", path.display()));
    };

    let candidates = scan_shadertoy_dir(&base_dir)?;
    let image_from_dir = candidates
        .get("image")
        .cloned()
        .or_else(|| candidates.get("mainimage").cloned());

    let image_path = match (selected_file.as_ref(), image_from_dir) {
        (Some(file), Some(image)) => {
            let key = normalize_name(file.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
            if key == "image" || key == "mainimage" {
                file.clone()
            } else if matches!(
                key.as_str(),
                "buffera" | "bufferb" | "bufferc" | "bufferd" | "common"
            ) {
                image
            } else {
                file.clone()
            }
        }
        (Some(file), None) => file.clone(),
        (None, Some(image)) => image,
        (None, None) => {
            return Err("multipass folder must contain Image.glsl (or Image.frag/.fs)".to_string())
        }
    };

    let buffer_paths: [Option<PathBuf>; SHADER_CHANNELS] = [
        candidates.get("buffera").cloned(),
        candidates.get("bufferb").cloned(),
        candidates.get("bufferc").cloned(),
        candidates.get("bufferd").cloned(),
    ];

    Ok(ShadertoySet {
        image_path,
        common_path: candidates.get("common").cloned(),
        buffer_paths,
        sound_path: candidates.get("sound").cloned(),
        asset_channels: discover_channel_assets(&base_dir),
    })
}

fn scan_shadertoy_dir(dir: &Path) -> Result<std::collections::HashMap<String, PathBuf>, String> {
    let mut out: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
    let rd = fs::read_dir(dir).map_err(|e| format!("read_dir({}): {e}", dir.display()))?;
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_shader_extension(&path) {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let key = normalize_name(stem);
        if matches!(
            key.as_str(),
            "image"
                | "mainimage"
                | "common"
                | "buffera"
                | "bufferb"
                | "bufferc"
                | "bufferd"
                | "sound"
        ) {
            // Prefer first match; users can keep only one of each.
            out.entry(key).or_insert(path);
        }
    }
    Ok(out)
}

fn is_shader_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "glsl" | "frag" | "fs")
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn discover_channel_assets(base_dir: &Path) -> [Option<PathBuf>; SHADER_CHANNELS] {
    std::array::from_fn(|ch| find_channel_asset(base_dir, ch))
}

fn find_channel_asset(base_dir: &Path, ch: usize) -> Option<PathBuf> {
    let assets_dir = base_dir.join("assets");
    for dir in [&assets_dir, base_dir] {
        if !dir.is_dir() {
            continue;
        }
        let Ok(rd) = fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let key = normalize_name(stem);
            if key == format!("ichannel{ch}") {
                return Some(path);
            }
        }
    }
    None
}
