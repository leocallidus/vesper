use crate::config::PatternDensity;
use glow::HasContext;
use gtk4::prelude::*;
use gtk4::{EventControllerMotion, GLArea};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

const SHADER_CHANNELS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SamplerKind {
    Tex2D,
    Cube,
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

pub fn build_shadertoy_area(shader_path: &str, density: PatternDensity) -> GLArea {
    let gl_area = GLArea::new();
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);
    gl_area.set_auto_render(false);
    gl_area.set_has_depth_buffer(false);
    gl_area.set_has_stencil_buffer(false);
    #[allow(deprecated)]
    gl_area.set_use_es(false);
    gl_area.set_required_version(3, 3);

    let shader_path = shader_path.trim().to_string();
    let resources: Rc<RefCell<Option<ShadertoyResources>>> = Rc::new(RefCell::new(None));

    let mouse_pos: Rc<RefCell<Option<(f32, f32)>>> = Rc::new(RefCell::new(None));
    {
        let mouse_pos = mouse_pos.clone();
        let gl_area_for_scale = gl_area.clone();
        let controller = EventControllerMotion::new();
        controller.connect_motion(move |_, x, y| {
            let scale = gl_area_for_scale.scale_factor().max(1) as f32;
            *mouse_pos.borrow_mut() = Some((x as f32 * scale, y as f32 * scale));
        });
        gl_area.add_controller(controller);
    }

    gl_area.add_tick_callback(|widget, _frame_clock| {
        widget.queue_render();
        glib::ControlFlow::Continue
    });

    {
        let resources = resources.clone();
        let shader_path = shader_path.clone();
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

            let black_cube_tex = match unsafe { create_black_cubemap_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create cubemap texture: {err}");
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

            let black_3d_tex = match unsafe { create_black_3d_texture(&gl) } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Shadertoy GL init failed: create 3d texture: {err}");
                    unsafe {
                        gl.delete_texture(black_cube_tex);
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

            *resources.borrow_mut() = Some(ShadertoyResources {
                _libgl: libgl,
                gl,
                image: ShadertoyProgram {
                    program: image_program,
                    uniforms: image_uniforms,
                    channel_kinds: image_channel_kinds,
                },
                buffers,
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
        let mouse_pos = mouse_pos.clone();
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

            let (mx, my) = mouse_pos.borrow().unwrap_or((0.0, 0.0));
            let mx = mx * (render_w as f32 / target_w.max(1.0));
            let my_from_top = my * (render_h as f32 / target_h.max(1.0));
            // Shadertoy iMouse uses origin at bottom-left.
            let my = (render_h as f32 - my_from_top).max(0.0);

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
                    let channel_tex = resolve_channel_textures(res, pass_idx, true);
                    let Some(state) = res.buffer_states[pass_idx].as_mut() else {
                        continue;
                    };
                    bind_channels(
                        &res.gl,
                        &channel_tex,
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
                        &channel_tex,
                        &prog.channel_kinds,
                        t,
                        render_w,
                        render_h,
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
                        year,
                        month,
                        day,
                        seconds,
                    );
                    res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }

                // Image pass: render into offscreen texture using current-frame buffer outputs.
                let image_channels = resolve_channel_textures(res, SHADER_CHANNELS, false);
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
                    t,
                    render_w,
                    render_h,
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
                    year,
                    month,
                    day,
                    seconds,
                );
                res.gl.draw_arrays(glow::TRIANGLES, 0, 3);

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
    out.push_str("void main() {\n");
    out.push_str("    mainImage(fragColor, gl_FragCoord.xy);\n");
    out.push_str("}\n");
    out
}

fn strip_shadertoy_version_lines(src: &str) -> String {
    // Shadertoy snippets sometimes include `#version ...`; our wrapper defines version already.
    src.lines()
        .filter(|line| !line.trim_start().starts_with("#version"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy)]
struct Uniforms {
    u_time: Option<glow::NativeUniformLocation>,
    u_time_delta: Option<glow::NativeUniformLocation>,
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

    for _ in 0..3 {
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

fn infer_channel_kinds_from_compile_log(
    wrapped_src: &str,
    compile_log: &str,
    kinds: &mut [SamplerKind; SHADER_CHANNELS],
) -> bool {
    // Common issue: some Shadertoy shaders sample iChannelN as a cubemap (`texture(iChannelN, vec3)`),
    // but our default channel type is `sampler2D`. When this happens, try recompiling the program
    // with that channel declared as `samplerCube`.
    if !compile_log.contains("texture(sampler2D, vec3)") {
        return false;
    }

    let src_lines: Vec<&str> = wrapped_src.lines().collect();
    let mut changed = false;

    for log_line in compile_log.lines() {
        if !log_line.contains("texture(sampler2D, vec3)") {
            continue;
        }

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
            if kinds[ch] != SamplerKind::Tex2D {
                continue;
            }
            if src_line.contains(&format!("iChannel{ch}")) {
                kinds[ch] = SamplerKind::Cube;
                changed = true;
            }
        }
    }

    if changed {
        return true;
    }

    // Fallback: if we couldn't locate the channel line reliably, promote all channels to `samplerCube`.
    // This keeps many cubemap-based shaders working without forcing users to edit code.
    let mut any = false;
    for ch in 0..SHADER_CHANNELS {
        if kinds[ch] == SamplerKind::Tex2D {
            kinds[ch] = SamplerKind::Cube;
            any = true;
        }
    }
    any
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
    textures_2d: &[glow::NativeTexture; SHADER_CHANNELS],
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
    black_cube_tex: glow::NativeTexture,
    black_3d_tex: glow::NativeTexture,
) {
    for (i, tex) in textures_2d.iter().enumerate() {
        gl.active_texture(glow::TEXTURE0 + i as u32);
        match channel_kinds[i] {
            SamplerKind::Tex2D => {
                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
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

fn resolve_channel_textures(
    res: &ShadertoyResources,
    pass_idx: usize,
    buffer_pass: bool,
) -> [glow::NativeTexture; SHADER_CHANNELS] {
    std::array::from_fn(|ch| {
        let Some(state) = res.buffer_states[ch].as_ref() else {
            return res.black_tex;
        };
        if buffer_pass {
            if ch < pass_idx && res.buffers[ch].is_some() {
                state.tex_next
            } else {
                state.tex_prev
            }
        } else if res.buffers[ch].is_some() {
            state.tex_next
        } else {
            res.black_tex
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
    if let Some(loc) = &uniforms.u_frame {
        gl.uniform_1_i32(Some(loc), frame);
    }
    if let Some(loc) = &uniforms.u_resolution {
        gl.uniform_3_f32(Some(loc), w, h, 1.0);
    }
    if let Some(loc) = &uniforms.u_mouse {
        gl.uniform_4_f32(Some(loc), mx, my, 0.0, 0.0);
    }
    if let Some(loc) = &uniforms.u_date {
        gl.uniform_4_f32(Some(loc), year, month, day, seconds);
    }
}

unsafe fn set_channel_uniforms(
    gl: &glow::Context,
    uniforms: &Uniforms,
    channels: &[glow::NativeTexture; SHADER_CHANNELS],
    channel_kinds: &[SamplerKind; SHADER_CHANNELS],
    t: f32,
    render_w: i32,
    render_h: i32,
    black_tex: glow::NativeTexture,
) {
    for i in 0..SHADER_CHANNELS {
        let is_black = channel_kinds[i] != SamplerKind::Tex2D || channels[i] == black_tex;
        let (cw, ch) = if is_black { (1.0, 1.0) } else { (render_w as f32, render_h as f32) };
        if let Some(loc) = &uniforms.u_channel_resolution[i] {
            gl.uniform_3_f32(Some(loc), cw, ch, 1.0);
        }
        if let Some(loc) = &uniforms.u_channel_time[i] {
            gl.uniform_1_f32(Some(loc), if is_black { 0.0 } else { t });
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
            "image" | "mainimage" | "common" | "buffera" | "bufferb" | "bufferc" | "bufferd"
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
