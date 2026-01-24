use crate::config::{AnimatedPattern, PatternDensity, PatternSpeed, PatternTheme};
use glow::HasContext;
use gtk4::prelude::*;
use gtk4::{EventControllerMotion, GLArea};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use std::time::SystemTime;

struct ReactionDiffusionSim {
    tex_a: glow::NativeTexture,
    tex_b: glow::NativeTexture,
    fbo_a: glow::NativeFramebuffer,
    fbo_b: glow::NativeFramebuffer,
    size: (i32, i32),
    front_is_a: bool,
}

struct GlResources {
    _libgl: libloading::Library,
    gl: glow::Context,
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
    u_time: Option<glow::NativeUniformLocation>,
    u_resolution: Option<glow::NativeUniformLocation>,
    u_pattern: Option<glow::NativeUniformLocation>,
    u_density: Option<glow::NativeUniformLocation>,
    u_theme: Option<glow::NativeUniformLocation>,
    u_mouse: Option<glow::NativeUniformLocation>,
    u_mouse_active: Option<glow::NativeUniformLocation>,
    u_seed: Option<glow::NativeUniformLocation>,
    u_pass: Option<glow::NativeUniformLocation>,
    u_state: Option<glow::NativeUniformLocation>,
    u_state_size: Option<glow::NativeUniformLocation>,
    u_bg: Option<glow::NativeUniformLocation>,
    u_bg_size: Option<glow::NativeUniformLocation>,
    u_bg_enabled: Option<glow::NativeUniformLocation>,
    bg_tex: Option<glow::NativeTexture>,
    bg_size: Option<(f32, f32)>,
    seed: i32,
    reaction_diffusion: Option<ReactionDiffusionSim>,
}

#[derive(Clone, Copy)]
struct GlPatternParams {
    pattern: AnimatedPattern,
    speed_mult: f32,
    density: PatternDensity,
    theme: PatternTheme,
}

pub fn build_gl_pattern_area(
    pattern: AnimatedPattern,
    speed: PatternSpeed,
    density: PatternDensity,
    theme: PatternTheme,
    water_ripples_bg_path: Option<&str>,
) -> GLArea {
    let gl_area = GLArea::new();
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);
    gl_area.set_auto_render(false);
    gl_area.set_has_depth_buffer(false);
    gl_area.set_has_stencil_buffer(false);
    #[allow(deprecated)]
    gl_area.set_use_es(false);
    gl_area.set_required_version(3, 3);

    let params = GlPatternParams {
        pattern,
        speed_mult: match speed {
            PatternSpeed::Slow => 0.5,
            PatternSpeed::Normal => 1.0,
            PatternSpeed::Fast => 2.0,
        },
        density,
        theme,
    };
    let water_ripples_bg_path = water_ripples_bg_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let start = Instant::now();
    let mouse_pos: Rc<RefCell<Option<(f32, f32)>>> = Rc::new(RefCell::new(None));
    let mouse_last_motion: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
    let resources: Rc<RefCell<Option<GlResources>>> = Rc::new(RefCell::new(None));

    {
        let mouse_pos = mouse_pos.clone();
        let mouse_last_motion = mouse_last_motion.clone();
        let gl_area_for_scale = gl_area.clone();
        let controller = EventControllerMotion::new();
        controller.connect_motion(move |_, x, y| {
            let scale = gl_area_for_scale.scale_factor().max(1) as f32;
            *mouse_pos.borrow_mut() = Some((x as f32 * scale, y as f32 * scale));
            *mouse_last_motion.borrow_mut() = Some(Instant::now());
        });
        gl_area.add_controller(controller);
    }

    {
        let resources = resources.clone();
        gl_area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() {
                return;
            }

            let libgl = match unsafe { libloading::Library::new("libGL.so.1") } {
                Ok(lib) => lib,
                Err(err) => {
                    eprintln!("GL init failed: unable to open libGL.so.1: {err}");
                    return;
                }
            };

            let gl = unsafe {
                glow::Context::from_loader_function(|name| {
                    let symbol = format!("{name}\0");
                    // Safety: the OpenGL loader returns a raw function pointer.
                    libgl
                        .get::<*const core::ffi::c_void>(symbol.as_bytes())
                        .map(|s| *s)
                        .unwrap_or(std::ptr::null())
                })
            };

            let (program, uniforms) = match unsafe { compile_program(&gl) } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("GL init failed: {err}");
                    return;
                }
            };

            let vao = match unsafe { gl.create_vertex_array() } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("GL init failed: create_vertex_array: {err}");
                    return;
                }
            };
            let vbo = match unsafe { gl.create_buffer() } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("GL init failed: create_buffer: {err}");
                    return;
                }
            };
            unsafe {
                gl.bind_vertex_array(Some(vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                let vertices: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    bytemuck::cast_slice(&vertices),
                    glow::STATIC_DRAW,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
            }

            let (bg_tex, bg_size) = if matches!(params.pattern, AnimatedPattern::WaterRipples) {
                water_ripples_bg_path
                    .as_deref()
                    .and_then(|path| load_texture_rgba8(&gl, path).ok())
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };

            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| (d.as_nanos() as u64) ^ (d.as_secs() << 32))
                .unwrap_or(0x1234_5678_9abc_def0);
            let seed = (seed ^ (seed >> 33) ^ (seed << 11)) as i32;

            *resources.borrow_mut() = Some(GlResources {
                _libgl: libgl,
                gl,
                program,
                vao,
                vbo,
                u_time: uniforms.u_time,
                u_resolution: uniforms.u_resolution,
                u_pattern: uniforms.u_pattern,
                u_density: uniforms.u_density,
                u_theme: uniforms.u_theme,
                u_mouse: uniforms.u_mouse,
                u_mouse_active: uniforms.u_mouse_active,
                u_seed: uniforms.u_seed,
                u_pass: uniforms.u_pass,
                u_state: uniforms.u_state,
                u_state_size: uniforms.u_state_size,
                u_bg: uniforms.u_bg,
                u_bg_size: uniforms.u_bg_size,
                u_bg_enabled: uniforms.u_bg_enabled,
                bg_tex,
                bg_size,
                seed,
                reaction_diffusion: None,
            });
        });
    }

    {
        let resources = resources.clone();
        gl_area.connect_unrealize(move |area| {
            if resources.borrow().is_none() {
                return;
            }
            area.make_current();
            if area.error().is_some() {
                *resources.borrow_mut() = None;
                return;
            }
            let Some(res) = resources.borrow_mut().take() else {
                return;
            };
            unsafe {
                if let Some(tex) = res.bg_tex {
                    res.gl.delete_texture(tex);
                }
                if let Some(sim) = res.reaction_diffusion {
                    res.gl.delete_framebuffer(sim.fbo_a);
                    res.gl.delete_framebuffer(sim.fbo_b);
                    res.gl.delete_texture(sim.tex_a);
                    res.gl.delete_texture(sim.tex_b);
                }
                res.gl.delete_program(res.program);
                res.gl.delete_buffer(res.vbo);
                res.gl.delete_vertex_array(res.vao);
            }
        });
    }

    {
        let resources = resources.clone();
        let mouse_pos = mouse_pos.clone();
        let mouse_last_motion = mouse_last_motion.clone();
        gl_area.connect_render(move |area, _context| {
            area.make_current();
            if area.error().is_some() {
                return glib::Propagation::Stop;
            }

            let mut borrowed = resources.borrow_mut();
            let Some(res) = borrowed.as_mut() else {
                return glib::Propagation::Stop;
            };

            let scale = area.scale_factor().max(1) as i32;
            #[allow(deprecated)]
            let fb_width = (area.allocated_width() * scale).max(1);
            #[allow(deprecated)]
            let fb_height = (area.allocated_height() * scale).max(1);

            let t = start.elapsed().as_secs_f32() * params.speed_mult;
            let (mouse_x, mouse_y) = mouse_pos
                .borrow()
                .unwrap_or((fb_width as f32 * 0.5, fb_height as f32 * 0.5));
            let mouse_active = mouse_last_motion
                .borrow()
                .map(|t| t.elapsed().as_secs_f32() < 3.0)
                .unwrap_or(false) as i32;

            unsafe {
                res.gl.disable(glow::DEPTH_TEST);
                res.gl.disable(glow::CULL_FACE);
                res.gl.disable(glow::BLEND);

                res.gl.use_program(Some(res.program));
                set_common_uniforms(
                    res,
                    t,
                    fb_width as f32,
                    fb_height as f32,
                    mouse_x,
                    mouse_y,
                    mouse_active,
                    params.pattern,
                    params.density,
                    params.theme,
                );

                if matches!(params.pattern, AnimatedPattern::ReactionDiffusion) {
                    let (sim_w, sim_h, steps) =
                        reaction_diffusion_sim_config(params.density, fb_width, fb_height);
                    let mouse_sim_x = mouse_x * (sim_w as f32) / (fb_width as f32);
                    let mouse_sim_y = mouse_y * (sim_h as f32) / (fb_height as f32);
                    if let Err(err) = ensure_reaction_diffusion_sim(
                        &res.gl,
                        &mut res.reaction_diffusion,
                        res.seed,
                        sim_w,
                        sim_h,
                    ) {
                        eprintln!("GL reaction-diffusion init failed: {err}");
                    }

                    if let Some(sim) = res.reaction_diffusion.as_mut() {
                        // Simulation passes (ping-pong).
                        if let Some(loc) = &res.u_pass {
                            res.gl.uniform_1_i32(Some(loc), 1);
                        }
                        if let Some(loc) = &res.u_pattern {
                            res.gl.uniform_1_i32(Some(loc), 19);
                        }
                        if let Some(loc) = &res.u_density {
                            res.gl
                                .uniform_1_f32(Some(loc), density_scale(params.density));
                        }
                        if let Some(loc) = &res.u_state {
                            res.gl.uniform_1_i32(Some(loc), 1);
                        }
                        if let Some(loc) = &res.u_state_size {
                            res.gl.uniform_2_f32(Some(loc), sim_w as f32, sim_h as f32);
                        }
                        if let Some(loc) = &res.u_mouse {
                            res.gl.uniform_2_f32(Some(loc), mouse_sim_x, mouse_sim_y);
                        }

                        for _ in 0..steps {
                            let (read_tex, write_fbo) = if sim.front_is_a {
                                (sim.tex_a, sim.fbo_b)
                            } else {
                                (sim.tex_b, sim.fbo_a)
                            };
                            res.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(write_fbo));
                            res.gl.viewport(0, 0, sim_w, sim_h);
                            if let Some(loc) = &res.u_resolution {
                                res.gl.uniform_2_f32(Some(loc), sim_w as f32, sim_h as f32);
                            }
                            if let Some(loc) = &res.u_mouse {
                                res.gl.uniform_2_f32(Some(loc), mouse_sim_x, mouse_sim_y);
                            }
                            res.gl.active_texture(glow::TEXTURE1);
                            res.gl.bind_texture(glow::TEXTURE_2D, Some(read_tex));

                            res.gl.bind_vertex_array(Some(res.vao));
                            res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                            res.gl.bind_vertex_array(None);

                            sim.front_is_a = !sim.front_is_a;
                        }

                        // Render to the screen.
                        res.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                        area.attach_buffers();
                        res.gl.viewport(0, 0, fb_width, fb_height);
                        if let Some(loc) = &res.u_pass {
                            res.gl.uniform_1_i32(Some(loc), 0);
                        }
                        if let Some(loc) = &res.u_pattern {
                            res.gl.uniform_1_i32(Some(loc), 19);
                        }
                        if let Some(loc) = &res.u_resolution {
                            res.gl
                                .uniform_2_f32(Some(loc), fb_width as f32, fb_height as f32);
                        }
                        if let Some(loc) = &res.u_mouse {
                            res.gl.uniform_2_f32(Some(loc), mouse_x, mouse_y);
                        }
                        if let Some(loc) = &res.u_state_size {
                            res.gl.uniform_2_f32(Some(loc), sim_w as f32, sim_h as f32);
                        }
                        let read_tex = if sim.front_is_a { sim.tex_a } else { sim.tex_b };
                        res.gl.active_texture(glow::TEXTURE1);
                        res.gl.bind_texture(glow::TEXTURE_2D, Some(read_tex));

                        res.gl.active_texture(glow::TEXTURE0);
                        res.gl.bind_texture(glow::TEXTURE_2D, res.bg_tex);

                        res.gl.bind_vertex_array(Some(res.vao));
                        res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                        res.gl.bind_vertex_array(None);
                    } else {
                        // Fallback: just draw black.
                        res.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                        area.attach_buffers();
                        res.gl.viewport(0, 0, fb_width, fb_height);
                        if let Some(loc) = &res.u_pass {
                            res.gl.uniform_1_i32(Some(loc), 0);
                        }
                        if let Some(loc) = &res.u_pattern {
                            res.gl.uniform_1_i32(Some(loc), 19);
                        }
                        if let Some(loc) = &res.u_resolution {
                            res.gl
                                .uniform_2_f32(Some(loc), fb_width as f32, fb_height as f32);
                        }
                        res.gl.active_texture(glow::TEXTURE0);
                        res.gl.bind_texture(glow::TEXTURE_2D, None);
                        res.gl.active_texture(glow::TEXTURE1);
                        res.gl.bind_texture(glow::TEXTURE_2D, None);

                        res.gl.bind_vertex_array(Some(res.vao));
                        res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                        res.gl.bind_vertex_array(None);
                    }
                } else {
                    // Normal (stateless) patterns.
                    res.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    area.attach_buffers();
                    res.gl.viewport(0, 0, fb_width, fb_height);
                    if let Some(loc) = &res.u_pass {
                        res.gl.uniform_1_i32(Some(loc), 0);
                    }
                    if let Some(loc) = &res.u_state_size {
                        res.gl.uniform_2_f32(Some(loc), 1.0, 1.0);
                    }
                    res.gl.active_texture(glow::TEXTURE0);
                    res.gl.bind_texture(glow::TEXTURE_2D, res.bg_tex);
                    res.gl.active_texture(glow::TEXTURE1);
                    res.gl.bind_texture(glow::TEXTURE_2D, None);

                    res.gl.bind_vertex_array(Some(res.vao));
                    res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                    res.gl.bind_vertex_array(None);
                }

                res.gl.use_program(None);
                res.gl.active_texture(glow::TEXTURE1);
                res.gl.bind_texture(glow::TEXTURE_2D, None);
                res.gl.active_texture(glow::TEXTURE0);
                res.gl.bind_texture(glow::TEXTURE_2D, None);
            }

            glib::Propagation::Stop
        });
    }

    gl_area.add_tick_callback(|widget, _frame_clock| {
        widget.queue_render();
        glib::ControlFlow::Continue
    });

    gl_area
}

fn animated_pattern_id(pattern: AnimatedPattern) -> i32 {
    match pattern {
        AnimatedPattern::Matrix => 0,
        AnimatedPattern::Stars => 1,
        AnimatedPattern::Geometry => 2,
        AnimatedPattern::Flowfield => 3,
        AnimatedPattern::Aurora => 4,
        AnimatedPattern::Plasma => 5,
        AnimatedPattern::Bokeh => 6,
        AnimatedPattern::Constellation => 7,
        AnimatedPattern::Lissajous => 8,
        AnimatedPattern::Waves => 9,
        AnimatedPattern::Voronoi => 10,
        AnimatedPattern::Scanline => 11,
        AnimatedPattern::Fireflies => 12,
        AnimatedPattern::SmokeInk => 13,
        AnimatedPattern::WaterRipples => 14,
        AnimatedPattern::MatrixRain3D => 15,
        AnimatedPattern::Lcars => 16,
        AnimatedPattern::Terminal => 17,
        AnimatedPattern::Fractals => 18,
        AnimatedPattern::ReactionDiffusion => 19,
    }
}

fn density_scale(density: PatternDensity) -> f32 {
    match density {
        PatternDensity::Low => 0.85,
        PatternDensity::Medium => 1.0,
        PatternDensity::High => 1.25,
    }
}

fn theme_id(theme: PatternTheme) -> i32 {
    match theme {
        PatternTheme::Default => 0,
        PatternTheme::Mono => 1,
        PatternTheme::Warm => 2,
        PatternTheme::Cool => 3,
        PatternTheme::Random => 4,
    }
}

unsafe fn set_common_uniforms(
    res: &mut GlResources,
    t: f32,
    width: f32,
    height: f32,
    mouse_x: f32,
    mouse_y: f32,
    mouse_active: i32,
    pattern: AnimatedPattern,
    density: PatternDensity,
    theme: PatternTheme,
) {
    if let Some(loc) = &res.u_time {
        res.gl.uniform_1_f32(Some(loc), t);
    }
    if let Some(loc) = &res.u_resolution {
        res.gl.uniform_2_f32(Some(loc), width, height);
    }
    if let Some(loc) = &res.u_pattern {
        res.gl
            .uniform_1_i32(Some(loc), animated_pattern_id(pattern));
    }
    if let Some(loc) = &res.u_density {
        res.gl.uniform_1_f32(Some(loc), density_scale(density));
    }
    if let Some(loc) = &res.u_theme {
        res.gl.uniform_1_i32(Some(loc), theme_id(theme));
    }
    if let Some(loc) = &res.u_mouse {
        res.gl.uniform_2_f32(Some(loc), mouse_x, mouse_y);
    }
    if let Some(loc) = &res.u_mouse_active {
        res.gl.uniform_1_i32(Some(loc), mouse_active);
    }
    if let Some(loc) = &res.u_seed {
        res.gl.uniform_1_i32(Some(loc), res.seed);
    }
    if let Some(loc) = &res.u_bg {
        res.gl.uniform_1_i32(Some(loc), 0);
    }
    if let Some(loc) = &res.u_state {
        res.gl.uniform_1_i32(Some(loc), 1);
    }
    if let Some(loc) = &res.u_bg_size {
        let (w, h) = res.bg_size.unwrap_or((1.0, 1.0));
        res.gl.uniform_2_f32(Some(loc), w, h);
    }
    if let Some(loc) = &res.u_bg_enabled {
        res.gl
            .uniform_1_i32(Some(loc), if res.bg_tex.is_some() { 1 } else { 0 });
    }
    if let Some(loc) = &res.u_pass {
        res.gl.uniform_1_i32(Some(loc), 0);
    }
}

fn reaction_diffusion_sim_config(
    density: PatternDensity,
    fb_width: i32,
    fb_height: i32,
) -> (i32, i32, i32) {
    let div = match density {
        PatternDensity::Low => 5,
        PatternDensity::Medium => 4,
        PatternDensity::High => 3,
    };
    let steps = match density {
        PatternDensity::Low => 6,
        PatternDensity::Medium => 9,
        PatternDensity::High => 12,
    };
    let w = (fb_width / div).max(96);
    let h = (fb_height / div).max(96);
    (w, h, steps)
}

fn rng_next_u32(state: &mut u64) -> u32 {
    // xorshift64*
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    ((x.wrapping_mul(2685821657736338717u64)) >> 32) as u32
}

unsafe fn create_rd_texture_rg32f(
    gl: &glow::Context,
    width: i32,
    height: i32,
    data_rg: &[f32],
) -> Result<glow::NativeTexture, String> {
    let tex = gl.create_texture().map_err(|e| e.to_string())?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
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
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RG32F as i32,
        width,
        height,
        0,
        glow::RG,
        glow::FLOAT,
        Some(bytemuck::cast_slice(data_rg)),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(tex)
}

unsafe fn create_fbo_for_texture(
    gl: &glow::Context,
    tex: glow::NativeTexture,
) -> Result<glow::NativeFramebuffer, String> {
    let fbo = gl.create_framebuffer().map_err(|e| e.to_string())?;
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
        return Err(format!("framebuffer incomplete: {status:#x}"));
    }
    Ok(fbo)
}

unsafe fn ensure_reaction_diffusion_sim(
    gl: &glow::Context,
    slot: &mut Option<ReactionDiffusionSim>,
    seed: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    if let Some(sim) = slot.as_ref() {
        if sim.size == (width, height) {
            return Ok(());
        }
    }

    if let Some(old) = slot.take() {
        gl.delete_framebuffer(old.fbo_a);
        gl.delete_framebuffer(old.fbo_b);
        gl.delete_texture(old.tex_a);
        gl.delete_texture(old.tex_b);
    }

    let mut data = vec![0.0f32; (width as usize) * (height as usize) * 2];
    for i in 0..(width as usize * height as usize) {
        data[i * 2] = 1.0; // U
        data[i * 2 + 1] = 0.0; // V
    }

    let mut rng = (seed as u32 as u64) ^ 0x9e3779b97f4a7c15u64;
    let spots = 6 + (rng_next_u32(&mut rng) % 12) as usize;
    let min_dim = width.min(height) as f32;
    for _ in 0..spots {
        let cx = (rng_next_u32(&mut rng) % (width as u32)) as i32;
        let cy = (rng_next_u32(&mut rng) % (height as u32)) as i32;
        let r = (min_dim * (0.012 + 0.030 * (rng_next_u32(&mut rng) as f32 / u32::MAX as f32)))
            .max(3.0);
        let r2 = r * r;
        let x0 = (cx as f32 - r).floor().max(0.0) as i32;
        let x1 = (cx as f32 + r).ceil().min((width - 1) as f32) as i32;
        let y0 = (cy as f32 - r).floor().max(0.0) as i32;
        let y1 = (cy as f32 + r).ceil().min((height - 1) as f32) as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 - cx as f32;
                let dy = y as f32 - cy as f32;
                if dx * dx + dy * dy <= r2 {
                    let idx = ((y * width + x) as usize) * 2;
                    data[idx] = 0.0;
                    data[idx + 1] = 1.0;
                }
            }
        }
    }

    // Tiny noise to break symmetry.
    for i in 0..(width as usize * height as usize) {
        let n = (rng_next_u32(&mut rng) as f32 / u32::MAX as f32) - 0.5;
        data[i * 2 + 1] = (data[i * 2 + 1] + n * 0.02).clamp(0.0, 1.0);
    }

    let tex_a = create_rd_texture_rg32f(gl, width, height, &data)?;
    let tex_b = create_rd_texture_rg32f(gl, width, height, &data)?;
    let fbo_a = create_fbo_for_texture(gl, tex_a)?;
    let fbo_b = create_fbo_for_texture(gl, tex_b)?;

    *slot = Some(ReactionDiffusionSim {
        tex_a,
        tex_b,
        fbo_a,
        fbo_b,
        size: (width, height),
        front_is_a: true,
    });
    Ok(())
}

fn load_texture_rgba8(
    gl: &glow::Context,
    path: &str,
) -> Result<(Option<glow::NativeTexture>, Option<(f32, f32)>), String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok((None, None));
    }

    let img = image::open(path).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    let width_i = i32::try_from(width).map_err(|_| "width too large".to_string())?;
    let height_i = i32::try_from(height).map_err(|_| "height too large".to_string())?;

    let tex = unsafe { gl.create_texture().map_err(|e| e.to_string())? };
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
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
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width_i,
            height_i,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            Some(&pixels),
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
        gl.bind_texture(glow::TEXTURE_2D, None);
    }

    Ok((Some(tex), Some((width as f32, height as f32))))
}

struct Uniforms {
    u_time: Option<glow::NativeUniformLocation>,
    u_resolution: Option<glow::NativeUniformLocation>,
    u_pattern: Option<glow::NativeUniformLocation>,
    u_density: Option<glow::NativeUniformLocation>,
    u_theme: Option<glow::NativeUniformLocation>,
    u_mouse: Option<glow::NativeUniformLocation>,
    u_mouse_active: Option<glow::NativeUniformLocation>,
    u_seed: Option<glow::NativeUniformLocation>,
    u_pass: Option<glow::NativeUniformLocation>,
    u_state: Option<glow::NativeUniformLocation>,
    u_state_size: Option<glow::NativeUniformLocation>,
    u_bg: Option<glow::NativeUniformLocation>,
    u_bg_size: Option<glow::NativeUniformLocation>,
    u_bg_enabled: Option<glow::NativeUniformLocation>,
}

unsafe fn compile_program(gl: &glow::Context) -> Result<(glow::NativeProgram, Uniforms), String> {
    let vertex_shader = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(vertex_shader, VERT_SRC);
    gl.compile_shader(vertex_shader);
    if !gl.get_shader_compile_status(vertex_shader) {
        let log = gl.get_shader_info_log(vertex_shader);
        gl.delete_shader(vertex_shader);
        return Err(format!("vertex shader compile failed: {log}"));
    }

    let fragment_shader = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(fragment_shader, FRAG_SRC);
    gl.compile_shader(fragment_shader);
    if !gl.get_shader_compile_status(fragment_shader) {
        let log = gl.get_shader_info_log(fragment_shader);
        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);
        return Err(format!("fragment shader compile failed: {log}"));
    }

    let program = gl.create_program().map_err(|e| e.to_string())?;
    gl.attach_shader(program, vertex_shader);
    gl.attach_shader(program, fragment_shader);
    gl.link_program(program);
    gl.detach_shader(program, vertex_shader);
    gl.detach_shader(program, fragment_shader);
    gl.delete_shader(vertex_shader);
    gl.delete_shader(fragment_shader);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("program link failed: {log}"));
    }

    let uniforms = Uniforms {
        u_time: gl.get_uniform_location(program, "u_time"),
        u_resolution: gl.get_uniform_location(program, "u_resolution"),
        u_pattern: gl.get_uniform_location(program, "u_pattern"),
        u_density: gl.get_uniform_location(program, "u_density"),
        u_theme: gl.get_uniform_location(program, "u_theme"),
        u_mouse: gl.get_uniform_location(program, "u_mouse"),
        u_mouse_active: gl.get_uniform_location(program, "u_mouse_active"),
        u_seed: gl.get_uniform_location(program, "u_seed"),
        u_pass: gl.get_uniform_location(program, "u_pass"),
        u_state: gl.get_uniform_location(program, "u_state"),
        u_state_size: gl.get_uniform_location(program, "u_state_size"),
        u_bg: gl.get_uniform_location(program, "u_bg"),
        u_bg_size: gl.get_uniform_location(program, "u_bg_size"),
        u_bg_enabled: gl.get_uniform_location(program, "u_bg_enabled"),
    };

    Ok((program, uniforms))
}

const VERT_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
out vec4 fragColor;

uniform vec2 u_resolution;
uniform float u_time;
uniform int u_pattern;
uniform float u_density;
uniform int u_theme;
uniform vec2 u_mouse;
uniform int u_mouse_active;
uniform int u_seed;
uniform int u_pass;
uniform sampler2D u_state;
uniform vec2 u_state_size;
uniform sampler2D u_bg;
uniform vec2 u_bg_size;
uniform int u_bg_enabled;

float hash11(float p) {
    p = fract(p * 0.1031);
    p *= p + 33.33;
    p *= p + p;
    return fract(p);
}

float hash21(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

vec2 hash22(vec2 p) {
    float n = hash21(p);
    return vec2(n, hash11(n + 1.234));
}

uint hash_u32(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}

float u32_to_01(uint x) {
    return float(x) * (1.0 / 4294967295.0);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    float a = hash21(i + vec2(0.0, 0.0));
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

float fbm(vec2 p) {
    float f = 0.0;
    float amp = 0.5;
    mat2 m = mat2(1.6, 1.2, -1.2, 1.6);
    for (int i = 0; i < 5; i++) {
        f += amp * noise(p);
        p = m * p;
        amp *= 0.5;
    }
    return f;
}

vec3 applyTheme(vec3 c) {
    if (u_theme == 1) {
        float g = dot(c, vec3(0.299, 0.587, 0.114));
        return vec3(g);
    }
    if (u_theme == 2) {
        float g = dot(c, vec3(0.299, 0.587, 0.114));
        return vec3(pow(g, 0.5), g * 0.7, g * 0.2);
    }
    if (u_theme == 3) {
        float g = dot(c, vec3(0.299, 0.587, 0.114));
        return vec3(g * 0.2, g * 0.7, pow(g, 0.5));
    }
    if (u_theme == 4) {
        return c.gbr;
    }
    return c;
}

float remEuclid(float x, float m) {
    return mod(mod(x, m) + m, m);
}

vec2 remEuclid2(vec2 x, vec2 m) {
    return mod(mod(x, m) + m, m);
}

float distToSegment(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

float aaFillCircle(vec2 p, vec2 c, float r) {
    float d = length(p - c) - r;
    float w = fwidth(d);
    return 1.0 - smoothstep(0.0, w, d);
}

float aaStrokeSegment(vec2 p, vec2 a, vec2 b, float halfWidth) {
    float d = distToSegment(p, a, b);
    float w = fwidth(d);
    return 1.0 - smoothstep(halfWidth - w, halfWidth + w, d);
}

float sdRoundRect(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

float aaRoundRect(vec2 p, vec2 center, vec2 halfSize, float radius) {
    float d = sdRoundRect(p - center, halfSize, radius);
    float w = fwidth(d) * 1.5;
    return 1.0 - smoothstep(0.0, w, d);
}

float aaRingSector(vec2 p, vec2 center, float r0, float r1, float a0, float a1) {
    vec2 v = p - center;
    float ang = atan(v.y, v.x);
    float dist = length(v);
    float angle = smoothstep(a0, a0 + 0.02, ang) * (1.0 - smoothstep(a1 - 0.02, a1, ang));
    float w0 = fwidth(dist) * 1.5;
    float ring = smoothstep(r0 + w0, r0, dist) * smoothstep(r1 - w0, r1, dist);
    return angle * ring;
}

float aaFillFromSdf(float d) {
    float w = fwidth(d) * 1.5;
    return 1.0 - smoothstep(0.0, w, d);
}

float aaStrokeFromSdf(float d, float halfWidth) {
    float ad = abs(d);
    float w = fwidth(ad) * 1.5;
    return 1.0 - smoothstep(halfWidth - w, halfWidth + w, ad);
}

float glowFromSdf(float d, float spreadPx) {
    return exp(-max(d, 0.0) / max(spreadPx, 1e-3));
}

vec3 patternPlasma(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    float x = uv.x * 6.28318;
    float y = uv.y * 6.28318;
    float v = 0.0;
    v += sin(x + t);
    v += sin(y + t * 1.3);
    v += sin(x + y + t * 0.7);
    vec2 p = uv * 3.0;
    v += sin(length(p - 1.5) * 4.0 - t * 1.5);
    v = v / 4.0;
    vec3 col = 0.5 + 0.5 * cos(6.28318 * (v + vec3(0.0, 0.33, 0.67)));
    col *= 0.7 + 0.3 * fbm(uv * (2.0 * u_density) + t * 0.15);
    return col;
}

vec3 patternWaves(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    float y = uv.y;
    float layers = 4.0 + 5.0 * (u_density - 0.85);
    vec3 col = vec3(0.02, 0.03, 0.06);
    for (int i = 0; i < 8; i++) {
        float fi = float(i);
        float w = fi / layers;
        float amp = mix(0.01, 0.06, w);
        float freq = mix(2.0, 10.0, w);
        float speed = mix(0.6, 1.8, w);
        float yy = 0.15 + w * 0.75;
        float wave = yy + amp * sin((uv.x * freq + t * speed) * 6.28318 + fi * 1.7);
        float d = abs(y - wave);
        float line = smoothstep(0.02, 0.0, d);
        vec3 lc = 0.5 + 0.5 * cos(vec3(0.2, 0.6, 0.9) * 6.28318 + fi * 0.7 + t * 0.2);
        col += lc * line * 0.8;
    }
    col += vec3(0.03) * fbm(uv * 3.0 + t * 0.1);
    return col;
}

vec3 patternStars(vec2 fragCoord, float t) {
    // Match Cairo: sparse stars, individual speed + radius + twinkle, drifting downwards.
    vec3 col = vec3(0.01, 0.01, 0.04);

    float mult = (u_density < 0.93) ? 0.5 : ((u_density < 1.12) ? 1.0 : 2.0);

    // Column-based star list: keeps per-fragment work bounded while preserving the Cairo look.
    float col_w = 50.0;
    int col_i = int(floor(fragCoord.x / col_w));
    float col_x0 = float(col_i) * col_w;

    float per_col_f = (u_resolution.y * col_w / 8500.0) * mult;
    int per_col = int(clamp(floor(per_col_f + 0.5), 1.0, 64.0));

    for (int j = 0; j < 64; j++) {
        if (j >= per_col) {
            break;
        }

        uint seed = uint(col_i) * 2654435761u ^ uint(j) * 2246822519u;
        uint h0 = hash_u32(seed);
        uint h1 = hash_u32(seed ^ 0x9e3779b9u);
        uint h2 = hash_u32(seed ^ 0x7f4a7c15u);
        uint h3 = hash_u32(seed ^ 0x85ebca6bu);
        uint h4 = hash_u32(seed ^ 0xc2b2ae35u);

        float x = col_x0 + u32_to_01(h0) * col_w;
        float y0 = u32_to_01(h1) * u_resolution.y;
        float speed = 20.0 + u32_to_01(h2) * 90.0;
        float radius = 0.6 + u32_to_01(h3) * 2.0;
        float y = mod(y0 + t * speed, u_resolution.y);

        float tw = sin(t * 2.0 + u32_to_01(h4) * 6.2831853) * 0.5 + 0.5;
        float brightness = 0.6 + 0.4 * tw;
        vec3 c = vec3(brightness, brightness, 1.0);

        float m = aaFillCircle(fragCoord, vec2(x, y), radius);
        col += c * m;
    }

    return col;
}

vec3 patternAurora(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    vec2 p = (fragCoord - 0.5 * u_resolution) / u_resolution.y;
    float y = uv.y;
    float bands = fbm(vec2(p.x * (2.2 * u_density) + t * 0.05, t * 0.02));
    float sweep = sin(p.x * 2.0 + t * 0.3) * 0.12 + 0.2;
    float a = smoothstep(0.6, 0.0, abs(y - (bands * 0.5 + 0.25 + sweep)));
    vec3 base = mix(vec3(0.02, 0.03, 0.06), vec3(0.0, 0.8, 0.5), a);
    base += vec3(0.0, 0.25, 0.4) * a * (0.5 + 0.5 * sin(t + p.x * 3.0));
    base *= 0.4 + 0.6 * smoothstep(0.0, 0.8, y);
    return base;
}

vec3 patternVoronoi(vec2 fragCoord, float t) {
    // Cairo-like "mosaic" voronoi: render in coarse cells (step=25px)
    float stepPx = 25.0;
    vec2 cell = floor(fragCoord / stepPx);
    vec2 p = cell * stepPx + vec2(stepPx * 0.5);

    int seedCount = (u_density < 0.93) ? 6 : ((u_density < 1.12) ? 12 : 20);

    float bestD2 = 1e30;
    vec3 bestCol = vec3(0.0);
    for (int i = 0; i < 20; i++) {
        if (i >= seedCount) {
            break;
        }
        float fi = float(i);
        float s = fi * 13.17 + 1.0;

        vec2 base = vec2(hash11(s), hash11(s + 1.0)) * u_resolution;
        vec2 vel = (-15.0 + 30.0 * vec2(hash11(s + 2.0), hash11(s + 3.0)));
        vec2 pos = remEuclid2(base + t * vel, u_resolution);

        vec3 seedCol = vec3(hash11(s + 4.0), hash11(s + 5.0), hash11(s + 6.0));

        vec2 d = p - pos;
        float d2 = dot(d, d);
        if (d2 < bestD2) {
            bestD2 = d2;
            bestCol = seedCol;
        }
    }

    float dist = sqrt(bestD2);
    float shade = max(0.2, 1.0 - dist / 300.0);
    vec3 col = bestCol * shade;

    // Draw seed points on top
    for (int i = 0; i < 20; i++) {
        if (i >= seedCount) {
            break;
        }
        float fi = float(i);
        float s = fi * 13.17 + 1.0;

        vec2 base = vec2(hash11(s), hash11(s + 1.0)) * u_resolution;
        vec2 vel = (-15.0 + 30.0 * vec2(hash11(s + 2.0), hash11(s + 3.0)));
        vec2 pos = remEuclid2(base + t * vel, u_resolution);

        float m = aaFillCircle(fragCoord, pos, 3.0);
        col = mix(col, vec3(1.0), m);
    }

    return col;
}

vec3 patternScanline(vec2 fragCoord, float t) {
    // Cairo-like "CRT" scanlines: rectangles + scan beam + vignette
    vec3 col = vec3(0.02, 0.02, 0.03);

    float scanHeight = (u_density < 0.93) ? 16.0 : ((u_density < 1.12) ? 10.0 : 6.0);
    float scanGap = 4.0;
    float totalH = scanHeight + scanGap;
    float rollOffset = remEuclid(t * 20.0, totalH);

    float aaY = fwidth(fragCoord.y);
    float localY = remEuclid(fragCoord.y - rollOffset, totalH);
    float scanMask = 1.0 - smoothstep(scanHeight - aaY, scanHeight + aaY, localY);
    col += vec3(0.0, 0.1, 0.0) * (0.2 * scanMask);

    float beamY = remEuclid(t * 200.0, u_resolution.y + 200.0) - 100.0;
    float beamTop = smoothstep(beamY - aaY, beamY + aaY, fragCoord.y);
    float beamBot = 1.0 - smoothstep(beamY + 80.0 - aaY, beamY + 80.0 + aaY, fragCoord.y);
    float beamMask = beamTop * beamBot;
    col += vec3(0.5, 0.0, 0.0) * (0.1 * beamMask);

    float border = 60.0;
    float edgeMask = 0.0;
    edgeMask = max(edgeMask, 1.0 - step(border, fragCoord.x));
    edgeMask = max(edgeMask, step(u_resolution.x - border, fragCoord.x));
    edgeMask = max(edgeMask, 1.0 - step(border, fragCoord.y));
    edgeMask = max(edgeMask, step(u_resolution.y - border, fragCoord.y));
    col *= 1.0 - 0.5 * edgeMask;

    return col;
}

vec3 patternFireflies(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    vec3 col = mix(vec3(0.01, 0.01, 0.02), vec3(0.02, 0.03, 0.05), uv.y);

    int count = (u_density < 0.93) ? 20 : ((u_density < 1.12) ? 50 : 100);

    for (int i = 0; i < 100; i++) {
        if (i >= count) {
            break;
        }
        float fi = float(i);
        float s = fi * 7.91 + 1.0;

        float xBase = hash11(s) * u_resolution.x;
        float yBase = hash11(s + 1.0) * u_resolution.y;

        float nx = hash11(s + 2.0) * 100.0;
        float ny = hash11(s + 3.0) * 100.0;

        float tOffset = hash11(s + 4.0) * 10.0;
        float tt = t * 0.5 + tOffset;

        float dx = sin(tt * 0.7 + nx) * 50.0 + cos(tt * 1.3) * 30.0;
        float dy = cos(tt * 0.5 + ny) * 50.0 + sin(tt * 1.7) * 30.0;

        vec2 pos = vec2(xBase + dx, yBase + dy);
        vec2 size = u_resolution + vec2(40.0);
        pos = remEuclid2(pos + vec2(20.0), size) - vec2(20.0);

        float pulseSpeed = 1.0 + hash11(s + 5.0);
        float pulse = sin(tt * pulseSpeed) * 0.5 + 0.5;

        float alpha = 0.3 + 0.7 * pulse;
        vec3 c = vec3(0.8 + 0.2 * pulse, 0.9 + 0.1 * pulse, 0.2);

        float d = length(fragCoord - pos);

        float glowR = 6.0 + 4.0 * pulse;
        float glow = exp(-(d * d) / (2.0 * glowR * glowR));
        col += c * (alpha * 0.3) * glow;

        float core = aaFillCircle(fragCoord, pos, 2.0);
        col += vec3(1.0, 1.0, 0.8) * alpha * core;
    }

    return col;
}

vec3 patternBokeh(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    float g = clamp((uv.x + uv.y) * 0.5, 0.0, 1.0);
    vec3 col = mix(vec3(0.05, 0.02, 0.05), vec3(0.1, 0.05, 0.1), g);

    int count = (u_density < 0.93) ? 20 : ((u_density < 1.12) ? 45 : 80);

    for (int i = 0; i < 80; i++) {
        if (i >= count) {
            break;
        }
        float fi = float(i);
        float s = fi * 9.83 + 1.0;

        float rBase = hash11(s);
        float gBase = hash11(s + 1.0);
        float bBase = hash11(s + 2.0);

        float radius = 20.0 + hash11(s + 3.0) * 80.0;
        float speedX = -10.0 + hash11(s + 4.0) * 20.0;
        float speedY = -10.0 + hash11(s + 5.0) * 20.0;
        float xStart = hash11(s + 6.0) * u_resolution.x;
        float yStart = hash11(s + 7.0) * u_resolution.y;

        float x = remEuclid(xStart + t * speedX, u_resolution.x + radius * 2.0) - radius;
        float y = remEuclid(yStart + t * speedY, u_resolution.y + radius * 2.0) - radius;
        vec2 center = vec2(x, y);

        float fadeSpeed = 0.2 + hash11(s + 8.0) * 0.5;
        float offset = hash11(s + 9.0) * 10.0;
        float alpha = 0.1 + (sin(t * fadeSpeed + offset) * 0.5 + 0.5) * 0.15;

        float hue = hash11(s + 10.0);
        vec3 c = (hue > 0.5)
            ? vec3(0.8 + rBase * 0.2, 0.4 + gBase * 0.3, 0.2)
            : vec3(0.1, 0.5 + gBase * 0.3, 0.7 + bBase * 0.3);

        float d = length(fragCoord - center);
        float edge = 1.0 - smoothstep(radius - fwidth(d), radius + fwidth(d), d);
        float interior = pow(clamp(1.0 - d / radius, 0.0, 1.0), 0.4);
        float mask = edge * (0.35 + 0.65 * interior);

        col += c * alpha * mask;
    }

    return col;
}

vec3 patternGeometry(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    vec2 p = (fragCoord - 0.5 * u_resolution) / u_resolution.y;
    float ang = t * 0.2;
    mat2 rot = mat2(cos(ang), -sin(ang), sin(ang), cos(ang));
    p = rot * p;
    float grid = 8.0 * u_density;
    vec2 g = abs(fract(p * grid) - 0.5);
    float lines = smoothstep(0.08, 0.0, min(g.x, g.y));
    float diag = smoothstep(0.02, 0.0, abs(g.x - g.y));
    vec3 col = vec3(0.02, 0.03, 0.05);
    col += vec3(0.1, 0.6, 0.9) * lines;
    col += vec3(0.9, 0.5, 0.2) * diag * 0.6;
    col += vec3(0.08) * fbm(uv * 4.0 + t * 0.1);
    return col;
}

vec3 patternLissajous(vec2 fragCoord, float t) {
    vec3 col = vec3(0.0, 0.0, 0.05);

    vec2 center = u_resolution * 0.5;
    float scale = min(u_resolution.x, u_resolution.y) * 0.4;

    float a = 3.0 + sin(t * 0.05) * 2.0;
    float b = 4.0 + cos(t * 0.03) * 3.0;
    float delta = t * 0.2;

    float tail = (u_density < 0.93) ? 10.0 : ((u_density < 1.12) ? 20.0 : 40.0);
    int samples = (u_density < 0.93) ? 32 : ((u_density < 1.12) ? 48 : 64);

    vec2 prev = center + scale * vec2(sin(a * t + delta), sin(b * t));
    for (int i = 1; i < 64; i++) {
        if (i >= samples) {
            break;
        }
        float u = float(i) / float(samples - 1);
        float tt = t - u * tail;
        vec2 p = center + scale * vec2(sin(a * tt + delta), sin(b * tt));

        float alpha = 1.0 - u;
        vec3 c = vec3(
            0.5 + 0.5 * sin(tt * 0.5),
            0.5 + 0.5 * sin(tt * 0.3 + 2.0),
            0.5 + 0.5 * sin(tt * 0.7 + 4.0)
        );

        float line = aaStrokeSegment(fragCoord, prev, p, 1.0);
        col += c * alpha * line;

        prev = p;
    }

    return col;
}

vec3 patternFlowfield(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;
    vec2 p = (fragCoord - 0.5 * u_resolution) / u_resolution.y;
    float n = fbm(p * (2.5 * u_density) + vec2(t * 0.05, t * 0.03));
    float ang = 6.28318 * n + t * 0.2;
    vec2 dir = vec2(cos(ang), sin(ang));
    float v = fbm((p + dir * 0.2) * (3.0 * u_density));
    float ink = smoothstep(0.55, 0.0, abs(v - 0.5));
    vec3 col = vec3(0.02, 0.02, 0.03);
    col += vec3(0.2, 0.9, 0.7) * ink;
    col += vec3(0.08) * v;
    return col;
}

vec3 patternConstellation(vec2 fragCoord, float t) {
    vec3 col = vec3(0.05, 0.05, 0.08);

    int pointCount = (u_density < 0.93) ? 40 : ((u_density < 1.12) ? 80 : 140);
    float connectDistance = (u_density < 0.93) ? 200.0 : ((u_density < 1.12) ? 150.0 : 100.0);

    float area = max(1.0, u_resolution.x * u_resolution.y);
    float cellSize = sqrt(area / float(pointCount));
    vec2 cell = floor(fragCoord / cellSize);

    vec3 lineCol = vec3(0.6, 0.8, 1.0);

    for (int oy = -1; oy <= 1; oy++) {
        for (int ox = -1; ox <= 1; ox++) {
            vec2 c = cell + vec2(float(ox), float(oy));

            vec2 rnd = hash22(c);
            vec2 drift = vec2(
                sin(t * (0.2 + 0.3 * rnd.x) + rnd.x * 6.0),
                cos(t * (0.2 + 0.3 * rnd.y) + rnd.y * 6.0)
            ) * (0.25 * cellSize);
            vec2 p = (c + rnd) * cellSize + drift;

            float star = aaFillCircle(fragCoord, p, 2.0);
            col += vec3(1.0) * (0.6 * star);

            vec2 dirs[4];
            dirs[0] = vec2(1.0, 0.0);
            dirs[1] = vec2(0.0, 1.0);
            dirs[2] = vec2(1.0, 1.0);
            dirs[3] = vec2(-1.0, 1.0);
            for (int k = 0; k < 4; k++) {
                vec2 cn = c + dirs[k];
                vec2 rnd2 = hash22(cn);
                vec2 drift2 = vec2(
                    sin(t * (0.2 + 0.3 * rnd2.x) + rnd2.x * 6.0),
                    cos(t * (0.2 + 0.3 * rnd2.y) + rnd2.y * 6.0)
                ) * (0.25 * cellSize);
                vec2 q = (cn + rnd2) * cellSize + drift2;

                float len = length(q - p);
                if (len < connectDistance) {
                    float alpha = (1.0 - len / connectDistance) * 0.4;
                    float line = aaStrokeSegment(fragCoord, p, q, 0.5);
                    col += lineCol * (alpha * line);
                }
            }
        }
    }

    return col;
}

vec3 patternMatrix(vec2 fragCoord, float t) {
    // Match Cairo: crisp falling blocks per column, with deterministic per-column params.
    vec3 col = vec3(0.0);

    float cell = (u_density < 0.93) ? 24.0 : ((u_density < 1.12) ? 16.0 : 12.0);
    float cell_w = cell * 0.75;

    int col_i = int(floor(fragCoord.x / cell));
    float x0 = float(col_i) * cell;
    float x_local = fragCoord.x - x0;
    if (x_local < 0.0 || x_local > cell_w) {
        return col;
    }

    uint seed = uint(col_i) * 1103515245u + 12345u;
    float speed = 40.0 + float(hash_u32(seed) % 70u);
    float offset = float(hash_u32(seed ^ 0x9e3779b9u) % 1000u);
    int tail = int(6u + (hash_u32(seed ^ 0x7f4a7c15u) % 14u));
    float head = mod(t * speed + offset, u_resolution.y + cell * float(tail));

    int i = int(floor((head - fragCoord.y + cell_w) / cell));
    if (i < 0 || i >= tail) {
        return col;
    }

    float y0 = head - float(i) * cell;
    float y_local = fragCoord.y - y0;
    if (y_local < 0.0 || y_local > cell_w) {
        return col;
    }

    float alpha = 1.0 - float(i) / float(tail);
    float green = 0.2 + 0.8 * alpha;

    // Slight AA to avoid shimmer without "blur".
    float ax = 1.0 - smoothstep(cell_w - fwidth(x_local), cell_w + fwidth(x_local), x_local);
    float ay = 1.0 - smoothstep(cell_w - fwidth(y_local), cell_w + fwidth(y_local), y_local);
    float m = ax * ay;

    col += vec3(0.0, green, 0.0) * m;
    return col;
}

// 5x7 pixel glyphs (rows are 5-bit masks).
const int GLYPH5X7_ROWS[112] = int[112](
    // 0
    14, 17, 19, 21, 25, 17, 14,
    // 1
    4, 12, 4, 4, 4, 4, 14,
    // 2
    14, 17, 1, 2, 4, 8, 31,
    // 3
    30, 1, 1, 14, 1, 1, 30,
    // 4
    2, 6, 10, 18, 31, 2, 2,
    // 5
    31, 16, 30, 1, 1, 17, 14,
    // 6
    6, 8, 16, 30, 17, 17, 14,
    // 7
    31, 1, 2, 4, 8, 8, 8,
    // 8
    14, 17, 17, 14, 17, 17, 14,
    // 9
    14, 17, 17, 15, 1, 2, 12,
    // 10: A
    4, 10, 17, 17, 31, 17, 17,
    // 11: H
    17, 17, 17, 31, 17, 17, 17,
    // 12: K
    17, 18, 20, 24, 20, 18, 17,
    // 13: N
    17, 25, 21, 19, 17, 17, 17,
    // 14: X
    17, 10, 4, 4, 4, 10, 17,
    // 15: Z
    31, 1, 2, 4, 8, 16, 31
);

float glyph5x7(vec2 xy01, uint glyph_id) {
    int gx = int(floor(xy01.x * 5.0));
    int gy = int(floor(xy01.y * 7.0));
    if (gx < 0 || gx >= 5 || gy < 0 || gy >= 7) {
        return 0.0;
    }
    int id = int(glyph_id & 15u);
    int rowMask = GLYPH5X7_ROWS[id * 7 + gy];
    int bit = (rowMask >> (4 - gx)) & 1;
    if (bit == 0) {
        return 0.0;
    }

    vec2 p = vec2(fract(xy01.x * 5.0), fract(xy01.y * 7.0));
    float ex = min(p.x, 1.0 - p.x);
    float ey = min(p.y, 1.0 - p.y);
    float e = min(ex, ey);
    float aa = max(fwidth(p.x), fwidth(p.y)) * 1.5;
    return smoothstep(0.0, aa, e);
}

vec3 matrixGlyphLayer(vec2 fragCoord, float t, float scale, uint layer_seed) {
    vec3 col = vec3(0.0);

    float cell = 16.0;
    float cell_w = cell * 0.78;

    int col_i = int(floor(fragCoord.x / cell));
    float x0 = float(col_i) * cell;
    float x_local = fragCoord.x - x0;
    if (x_local < 0.0 || x_local > cell_w) {
        return col;
    }

    uint seed = uint(col_i) * 1103515245u + 12345u + layer_seed;
    float speed = (60.0 + float(hash_u32(seed) % 90u)) / max(scale, 0.001);
    float offset = float(hash_u32(seed ^ 0x9e3779b9u) % 1000u);
    int tail = int(8u + (hash_u32(seed ^ 0x7f4a7c15u) % 18u));
    float h = mod(t * speed + offset, (u_resolution.y * scale) + cell * float(tail));

    int i = int(floor((h - fragCoord.y + cell_w) / cell));
    if (i < 0 || i >= tail) {
        return col;
    }

    float y0 = h - float(i) * cell;
    float y_local = fragCoord.y - y0;
    if (y_local < 0.0 || y_local > cell_w) {
        return col;
    }

    float alpha = 1.0 - float(i) / float(tail);

    uint glyph_id = hash_u32(seed ^ uint(i) * 2246822519u ^ uint(int(floor(y0 / cell))) * 3266489917u);
    float m = glyph5x7(vec2(x_local / cell_w, y_local / cell_w), glyph_id);
    if (m <= 0.0) {
        return col;
    }

    float head = (i == 0) ? 1.0 : 0.0;
    vec3 glyph_col = mix(vec3(0.0, 0.35 + 0.65 * alpha, 0.0), vec3(0.7, 1.0, 0.75), head);

    // Subtle glow in the cell.
    vec2 c = vec2(cell_w * 0.5);
    float d = length(vec2(x_local, y_local) - c) / (cell_w * 0.65);
    float glow = exp(-d * d * 2.5);

    col += glyph_col * (m * (0.65 + 0.35 * glow));
    return col;
}

vec3 patternMatrixRain3D(vec2 fragCoord, float t) {
    vec2 center = u_resolution * 0.5;
    vec2 q = fragCoord - center;

    // Camera sway.
    float yaw = sin(t * 0.12) * 0.14;
    float pitch = cos(t * 0.10) * 0.05;

    vec3 col = vec3(0.0, 0.0, 0.0);
    int layers = 3;
    for (int li = 0; li < 3; li++) {
        float lf = float(li) / float(layers - 1);
        float depth = mix(0.0, 1.0, lf);
        float scale = mix(1.0, 2.3, depth);

        vec2 qq = q;
        qq.x += (qq.y + pitch * u_resolution.y) * (0.12 + 0.08 * depth);
        qq.x += sin(t * (0.06 + 0.02 * depth)) * (40.0 + 40.0 * depth);
        qq = mat2(cos(yaw), -sin(yaw), sin(yaw), cos(yaw)) * qq;

        vec2 fc = center + qq * scale;
        vec3 layer = matrixGlyphLayer(fc, t, scale, uint(li) * 0x9e3779b9u);

        // Depth fog + brightness.
        float fog = exp(-depth * 1.2);
        col += layer * fog;
    }

    // Vignette for depth.
    vec2 uv = fragCoord / u_resolution;
    float v = smoothstep(1.25, 0.35, length(uv - 0.5));
    col *= v;

    return col;
}

vec3 patternSmokeInk(vec2 fragCoord, float t) {
    // Procedural "smoke/ink" with mouse-reactive swirl (stateless, Shadertoy-style).
    vec2 uv = (fragCoord - 0.5 * u_resolution) / u_resolution.y;
    vec2 muv = (u_mouse - 0.5 * u_resolution) / u_resolution.y;

    float densityMult = mix(1.0, 1.5, clamp(u_density - 0.85, 0.0, 1.0));

    // Base curl field.
    vec2 p = uv * (2.0 * densityMult);
    float e = 0.08;
    float n1 = fbm(p + vec2(0.0, e) + t * 0.12);
    float n2 = fbm(p - vec2(0.0, e) + t * 0.12);
    float n3 = fbm(p + vec2(e, 0.0) + t * 0.12);
    float n4 = fbm(p - vec2(e, 0.0) + t * 0.12);
    vec2 curl = vec2(n1 - n2, -(n3 - n4)) / (2.0 * e);

    // Mouse acts as a source and a local vortex.
    float source = 0.0;
    if (u_mouse_active != 0) {
        float d = length(uv - muv);
        source = exp(-d * 7.0);
        vec2 r = uv - muv;
        float inv = 1.0 / max(dot(r, r), 0.002);
        vec2 vortex = vec2(-r.y, r.x) * inv;
        curl += vortex * source * 0.6;
    } else {
        // Gentle idle source at center.
        float d = length(uv);
        source = 0.25 * exp(-d * 3.0);
    }

    // Advect coords through the flow to create wispy shapes.
    vec2 q = p + curl * (0.9 + 0.6 * source);
    float f = fbm(q * 1.8 + t * 0.25);
    float g = fbm(q * 3.7 - t * 0.18);
    float smoke = smoothstep(0.35, 0.95, f * 0.65 + g * 0.35 + 0.25 * source);

    // Color: ink-like.
    vec3 bg = vec3(0.01, 0.01, 0.02);
    vec3 ink = vec3(0.15, 0.45, 0.95);
    vec3 col = mix(bg, ink, smoke * 0.9);
    col += vec3(0.04, 0.02, 0.06) * (1.0 - smoke) * 0.6;
    return col;
}

vec2 coverUV(vec2 uv, vec2 srcSize, vec2 dstSize) {
    float srcAspect = srcSize.x / max(srcSize.y, 1.0);
    float dstAspect = dstSize.x / max(dstSize.y, 1.0);
    vec2 outUV = uv;
    if (srcAspect > dstAspect) {
        // Wider source -> scale X (crop left/right).
        float s = srcAspect / dstAspect;
        outUV.x = (uv.x - 0.5) * s + 0.5;
    } else {
        // Taller source -> scale Y (crop top/bottom).
        float s = dstAspect / srcAspect;
        outUV.y = (uv.y - 0.5) * s + 0.5;
    }
    return clamp(outUV, 0.0, 1.0);
}

vec3 sampleBackground(vec2 uv) {
    if (u_bg_enabled == 0) {
        vec3 top = vec3(0.06, 0.10, 0.16);
        vec3 bot = vec3(0.01, 0.01, 0.02);
        float v = smoothstep(0.0, 1.0, uv.y);
        return mix(bot, top, v);
    }
    vec2 cuv = coverUV(uv, u_bg_size, u_resolution);
    // Loaded images are "top-left origin", while OpenGL UVs are "bottom-left origin".
    cuv.y = 1.0 - cuv.y;
    return texture(u_bg, cuv).rgb;
}

float ripple(vec2 p, vec2 c, float t, float freq, float speed, float decay) {
    float d = length(p - c);
    float w = sin(d * freq - t * speed);
    float amp = exp(-d * decay) / (1.0 + d * 8.0);
    return w * amp;
}

float waterHeight(vec2 uv, float t) {
    float aspect = u_resolution.x / max(u_resolution.y, 1.0);
    vec2 p = (uv - 0.5) * vec2(aspect, 1.0);
    float k = 1.0 + 0.9 * clamp(u_density - 0.85, 0.0, 1.0);

    float w = 0.0;

    // A few gentle ripple sources + subtle noise for realism.
    w += ripple(p, vec2(-0.25, -0.10), t, 18.0 * k, 2.4, 2.2) * 0.90;
    w += ripple(p, vec2(0.30, 0.18), t, 22.0 * k, 1.9, 2.0) * 0.80;
    w += ripple(p, vec2(0.05, -0.25), t, 15.0 * k, 2.1, 2.6) * 0.70;

    w += (fbm(p * (2.4 * k) + t * 0.10) - 0.5) * 0.25;

    return w;
}

vec3 patternWaterRipples(vec2 fragCoord, float t) {
    vec2 uv = fragCoord / u_resolution;

    float h = waterHeight(uv, t);
    vec2 grad = vec2(dFdx(h), dFdy(h));
    vec3 n = normalize(vec3(-grad.x, -grad.y, 0.25));

    float refrStrength = 0.02;
    vec2 refr = n.xy * refrStrength;
    vec3 base = sampleBackground(uv + refr);

    // Caustics-ish modulation.
    float caust = 0.5 + 0.5 * sin((h * 2.4 + t * 0.6) * 6.28318);
    base *= 0.97 + 0.06 * caust;

    vec3 lightDir = normalize(vec3(-0.3, 0.4, 1.0));
    float diff = clamp(dot(n, normalize(vec3(0.0, 0.0, 1.0))), 0.0, 1.0);
    vec3 halfV = normalize(lightDir + vec3(0.0, 0.0, 1.0));
    float spec = pow(max(dot(n, halfV), 0.0), 160.0);
    float fres = pow(1.0 - diff, 2.2);

    vec3 col = base;
    col += vec3(0.18, 0.22, 0.26) * spec;
    col = mix(col, col + vec3(0.05, 0.08, 0.12), 0.22 * fres);
    return col;
}

// Terminal: fake logs / "hacking" text sequences.
// Minimal 5x7 font (rows are 5-bit masks, top->bottom).
const int TERM_FONT_ROWS[315] = int[315](
    // 0: ' ' (space)
    0, 0, 0, 0, 0, 0, 0,
    // 1: '0'
    14, 17, 19, 21, 25, 17, 14,
    // 2: '1'
    4, 12, 4, 4, 4, 4, 14,
    // 3: '2'
    14, 17, 1, 2, 4, 8, 31,
    // 4: '3'
    30, 1, 1, 14, 1, 1, 30,
    // 5: '4'
    2, 6, 10, 18, 31, 2, 2,
    // 6: '5'
    31, 16, 30, 1, 1, 17, 14,
    // 7: '6'
    6, 8, 16, 30, 17, 17, 14,
    // 8: '7'
    31, 1, 2, 4, 8, 8, 8,
    // 9: '8'
    14, 17, 17, 14, 17, 17, 14,
    // 10: '9'
    14, 17, 17, 15, 1, 2, 12,
    // 11: 'A'
    4, 10, 17, 17, 31, 17, 17,
    // 12: 'B'
    30, 17, 17, 30, 17, 17, 30,
    // 13: 'C'
    14, 17, 16, 16, 16, 17, 14,
    // 14: 'D'
    30, 17, 17, 17, 17, 17, 30,
    // 15: 'E'
    31, 16, 16, 30, 16, 16, 31,
    // 16: 'F'
    31, 16, 16, 30, 16, 16, 16,
    // 17: 'G'
    14, 17, 16, 23, 17, 17, 14,
    // 18: 'H'
    17, 17, 17, 31, 17, 17, 17,
    // 19: 'I'
    14, 4, 4, 4, 4, 4, 14,
    // 20: 'J'
    1, 1, 1, 1, 17, 17, 14,
    // 21: 'K'
    17, 18, 20, 24, 20, 18, 17,
    // 22: 'L'
    16, 16, 16, 16, 16, 16, 31,
    // 23: 'M'
    17, 27, 21, 21, 17, 17, 17,
    // 24: 'N'
    17, 25, 21, 19, 17, 17, 17,
    // 25: 'O'
    14, 17, 17, 17, 17, 17, 14,
    // 26: 'P'
    30, 17, 17, 30, 16, 16, 16,
    // 27: 'Q'
    14, 17, 17, 17, 21, 18, 13,
    // 28: 'R'
    30, 17, 17, 30, 20, 18, 17,
    // 29: 'S'
    15, 16, 16, 14, 1, 1, 30,
    // 30: 'T'
    31, 4, 4, 4, 4, 4, 4,
    // 31: 'U'
    17, 17, 17, 17, 17, 17, 14,
    // 32: 'V'
    17, 17, 17, 17, 17, 10, 4,
    // 33: 'W'
    17, 17, 17, 21, 21, 21, 10,
    // 34: 'X'
    17, 10, 4, 4, 4, 10, 17,
    // 35: 'Y'
    17, 10, 4, 4, 4, 4, 4,
    // 36: 'Z'
    31, 1, 2, 4, 8, 16, 31,
    // 37: '['
    28, 16, 16, 16, 16, 16, 28,
    // 38: ']'
    7, 1, 1, 1, 1, 1, 7,
    // 39: ':'
    0, 4, 0, 0, 4, 0, 0,
    // 40: '.'
    0, 0, 0, 0, 0, 0, 4,
    // 41: '-'
    0, 0, 0, 14, 0, 0, 0,
    // 42: '/'
    1, 2, 4, 8, 16, 0, 0,
    // 43: '>'
    0, 8, 4, 2, 4, 8, 0,
    // 44: '_'
    0, 0, 0, 0, 0, 0, 31
);

int termGlyphIndex(int c) {
    // Map lowercase to uppercase.
    if (c >= 97 && c <= 122) {
        c -= 32;
    }
    if (c == 32) return 0; // space

    // digits
    if (c >= 48 && c <= 57) {
        return 1 + (c - 48);
    }

    // letters
    if (c >= 65 && c <= 90) {
        return 11 + (c - 65);
    }

    if (c == 91) return 37; // [
    if (c == 93) return 38; // ]
    if (c == 58) return 39; // :
    if (c == 46) return 40; // .
    if (c == 45) return 41; // -
    if (c == 47) return 42; // /
    if (c == 62) return 43; // >
    if (c == 95) return 44; // _

    return 0;
}

float termGlyph5x7(vec2 xy01, int ch) {
    int gx = int(floor(xy01.x * 5.0));
    int gy = int(floor(xy01.y * 7.0));
    if (gx < 0 || gx >= 5 || gy < 0 || gy >= 7) {
        return 0.0;
    }

    int id = termGlyphIndex(ch);
    int rowMask = TERM_FONT_ROWS[id * 7 + gy];
    int bit = (rowMask >> (4 - gx)) & 1;
    if (bit == 0) {
        return 0.0;
    }

    vec2 p = vec2(fract(xy01.x * 5.0), fract(xy01.y * 7.0));
    float ex = min(p.x, 1.0 - p.x);
    float ey = min(p.y, 1.0 - p.y);
    float e = min(ex, ey);
    float aa = max(fwidth(p.x), fwidth(p.y)) * 1.25;
    return smoothstep(0.0, aa, e);
}

int emitWord(int col, int start, int wordId) {
    // Words are emitted via a compact switch to avoid big string tables.
    int i = col - start;
    if (i < 0) return -1;

    // 0 INFO
    if (wordId == 0) {
        if (i >= 4) return -1;
        if (i == 0) return 73;
        if (i == 1) return 78;
        if (i == 2) return 70;
        return 79;
    }
    // 1 WARN
    if (wordId == 1) {
        if (i >= 4) return -1;
        if (i == 0) return 87;
        if (i == 1) return 65;
        if (i == 2) return 82;
        return 78;
    }
    // 2 ERROR
    if (wordId == 2) {
        if (i >= 5) return -1;
        if (i == 0) return 69;
        if (i == 1) return 82;
        if (i == 2) return 82;
        if (i == 3) return 79;
        return 82;
    }
    // 3 LOADED
    if (wordId == 3) {
        if (i >= 6) return -1;
        if (i == 0) return 76;
        if (i == 1) return 79;
        if (i == 2) return 65;
        if (i == 3) return 68;
        if (i == 4) return 69;
        return 68;
    }
    // 4 MODULE
    if (wordId == 4) {
        if (i >= 6) return -1;
        if (i == 0) return 77;
        if (i == 1) return 79;
        if (i == 2) return 68;
        if (i == 3) return 85;
        if (i == 4) return 76;
        return 69;
    }
    // 5 TIMEOUT
    if (wordId == 5) {
        if (i >= 7) return -1;
        if (i == 0) return 84;
        if (i == 1) return 73;
        if (i == 2) return 77;
        if (i == 3) return 69;
        if (i == 4) return 79;
        if (i == 5) return 85;
        return 84;
    }
    // 6 RETRY
    if (wordId == 6) {
        if (i >= 5) return -1;
        if (i == 0) return 82;
        if (i == 1) return 69;
        if (i == 2) return 84;
        if (i == 3) return 82;
        return 89;
    }
    // 7 AUTH
    if (wordId == 7) {
        if (i >= 4) return -1;
        if (i == 0) return 65;
        if (i == 1) return 85;
        if (i == 2) return 84;
        return 72;
    }
    // 8 DENIED
    if (wordId == 8) {
        if (i >= 6) return -1;
        if (i == 0) return 68;
        if (i == 1) return 69;
        if (i == 2) return 78;
        if (i == 3) return 73;
        if (i == 4) return 69;
        return 68;
    }
    // 9 CODE
    if (wordId == 9) {
        if (i >= 4) return -1;
        if (i == 0) return 67;
        if (i == 1) return 79;
        if (i == 2) return 68;
        return 69;
    }
    // 10 GET
    if (wordId == 10) {
        if (i >= 3) return -1;
        if (i == 0) return 71;
        if (i == 1) return 69;
        return 84;
    }
    // 11 API
    if (wordId == 11) {
        if (i >= 3) return -1;
        if (i == 0) return 65;
        if (i == 1) return 80;
        return 73;
    }
    // 12 V1
    if (wordId == 12) {
        if (i >= 2) return -1;
        if (i == 0) return 86;
        return 49;
    }
    // 13 OK
    if (wordId == 13) {
        if (i >= 2) return -1;
        if (i == 0) return 79;
        return 75;
    }
    // 14 SSH
    if (wordId == 14) {
        if (i >= 3) return -1;
        if (i == 0) return 83;
        if (i == 1) return 83;
        return 72;
    }
    // 15 CONNECTED
    if (wordId == 15) {
        if (i >= 9) return -1;
        if (i == 0) return 67;
        if (i == 1) return 79;
        if (i == 2) return 78;
        if (i == 3) return 78;
        if (i == 4) return 69;
        if (i == 5) return 67;
        if (i == 6) return 84;
        if (i == 7) return 69;
        return 68;
    }
    // 16 SCAN
    if (wordId == 16) {
        if (i >= 4) return -1;
        if (i == 0) return 83;
        if (i == 1) return 67;
        if (i == 2) return 65;
        return 78;
    }
    // 17 OPEN
    if (wordId == 17) {
        if (i >= 4) return -1;
        if (i == 0) return 79;
        if (i == 1) return 80;
        if (i == 2) return 69;
        return 78;
    }
    // 18 DECRYPT
    if (wordId == 18) {
        if (i >= 7) return -1;
        if (i == 0) return 68;
        if (i == 1) return 69;
        if (i == 2) return 67;
        if (i == 3) return 82;
        if (i == 4) return 89;
        if (i == 5) return 80;
        return 84;
    }
    // 19 BLOCK
    if (wordId == 19) {
        if (i >= 5) return -1;
        if (i == 0) return 66;
        if (i == 1) return 76;
        if (i == 2) return 79;
        if (i == 3) return 67;
        return 75;
    }
    // 20 MS
    if (wordId == 20) {
        if (i >= 2) return -1;
        if (i == 0) return 77;
        return 83;
    }
    return -1;
}

int emitDecCharFixed(int col, int start, int value, int digits) {
    int i = col - start;
    if (i < 0 || i >= digits) return -1;
    int pow10 = 1;
    for (int k = 0; k < 6; k++) {
        if (k >= (digits - 1 - i)) break;
        pow10 *= 10;
    }
    int d = (value / pow10) % 10;
    return 48 + d;
}

int emitTimeChar(int col, int start, int lineId) {
    int i = col - start;
    if (i < 0 || i >= 8) return -1;

    int tt = lineId * 7;
    int ss = tt % 60;
    int mm = (tt / 60) % 60;
    int hh = (tt / 3600) % 24;

    if (i == 2 || i == 5) return 58; // :
    if (i == 0) return 48 + (hh / 10);
    if (i == 1) return 48 + (hh % 10);
    if (i == 3) return 48 + (mm / 10);
    if (i == 4) return 48 + (mm % 10);
    if (i == 6) return 48 + (ss / 10);
    return 48 + (ss % 10);
}

int emitHexCharFixed(int col, int start, uint value, int digits) {
    int i = col - start;
    if (i < 0 || i >= digits) return -1;
    int sh = (digits - 1 - i) * 4;
    uint n = (value >> uint(sh)) & 15u;
    if (n < 10u) return 48 + int(n);
    return 65 + int(n - 10u);
}

int emitIpCharFixed(int col, int start, uint seed) {
    int i = col - start;
    if (i < 0 || i >= 15) return -1;

    uint h0 = hash_u32(seed ^ 0x1234u);
    uint h1 = hash_u32(seed ^ 0x9e37u);
    uint h2 = hash_u32(seed ^ 0x7f4au);
    uint h3 = hash_u32(seed ^ 0x85ebu);
    int a = int(h0 % 256u);
    int b = int(h1 % 256u);
    int c = int(h2 % 256u);
    int d = int(h3 % 256u);

    if (i == 3 || i == 7 || i == 11) return 46; // .

    int seg = 0;
    int j = i;
    if (i >= 12) { seg = 3; j = i - 12; }
    else if (i >= 8) { seg = 2; j = i - 8; }
    else if (i >= 4) { seg = 1; j = i - 4; }

    int v = (seg == 0) ? a : ((seg == 1) ? b : ((seg == 2) ? c : d));
    return emitDecCharFixed(j, 0, v, 3);
}

int termLineChar(int lineId, int col, int cols, float t, uint seed) {
    // Keep empty margins on wide screens.
    int leftPad = int(clamp(floor(float(cols) * 0.06), 0.0, 6.0));
    int x = col - leftPad;
    if (x < 0) return 32;

    int typ = int(seed % 7u);

    // Type 0: [INFO] HH:MM:SS LOADED MODULE 0Xhhhhhhhh
    if (typ == 0) {
        if (x == 0) return 91; // [
        int w = emitWord(x, 1, 0);
        if (w != -1) return w;
        if (x == 5) return 93; // ]
        if (x == 6) return 32;
        int tc = emitTimeChar(x, 7, lineId);
        if (tc != -1) return tc;
        if (x == 15) return 32;
        w = emitWord(x, 16, 3); // LOADED
        if (w != -1) return w;
        if (x == 22) return 32;
        w = emitWord(x, 23, 4); // MODULE
        if (w != -1) return w;
        if (x == 29) return 32;
        if (x == 30) return 48; // 0
        if (x == 31) return 88; // X
        int hc = emitHexCharFixed(x, 32, hash_u32(seed ^ 0x3c6ef35fu), 8);
        if (hc != -1) return hc;
        return 32;
    }

    // Type 1: [WARN] HH:MM:SS TIMEOUT RETRY NN
    if (typ == 1) {
        if (x == 0) return 91;
        int w = emitWord(x, 1, 1); // WARN
        if (w != -1) return w;
        if (x == 5) return 93;
        if (x == 6) return 32;
        int tc = emitTimeChar(x, 7, lineId);
        if (tc != -1) return tc;
        if (x == 15) return 32;
        w = emitWord(x, 16, 5); // TIMEOUT
        if (w != -1) return w;
        if (x == 23) return 32;
        w = emitWord(x, 24, 6); // RETRY
        if (w != -1) return w;
        if (x == 29) return 32;
        int n = int(hash_u32(seed ^ 0x7f4a7c15u) % 100u);
        int dc = emitDecCharFixed(x, 30, n, 2);
        if (dc != -1) return dc;
        return 32;
    }

    // Type 2: [ERROR] HH:MM:SS AUTH DENIED CODE NNN
    if (typ == 2) {
        if (x == 0) return 91;
        int w = emitWord(x, 1, 2); // ERROR
        if (w != -1) return w;
        if (x == 6) return 93;
        if (x == 7) return 32;
        int tc = emitTimeChar(x, 8, lineId);
        if (tc != -1) return tc;
        if (x == 16) return 32;
        w = emitWord(x, 17, 7); // AUTH
        if (w != -1) return w;
        if (x == 21) return 32;
        w = emitWord(x, 22, 8); // DENIED
        if (w != -1) return w;
        if (x == 28) return 32;
        w = emitWord(x, 29, 9); // CODE
        if (w != -1) return w;
        if (x == 33) return 32;
        int code = int(100u + (hash_u32(seed ^ 0x85ebca6bu) % 900u));
        int dc = emitDecCharFixed(x, 34, code, 3);
        if (dc != -1) return dc;
        return 32;
    }

    // The remaining types use a prompt-ish prefix.
    if (x == 0) return 62; // >
    if (x == 1) return 32;

    // Type 3: > GET /API/V1/hhhhhhhh 200 OK NNNMS
    if (typ == 3) {
        int w = emitWord(x, 2, 10); // GET
        if (w != -1) return w;
        if (x == 5) return 32;
        if (x == 6) return 47; // /
        w = emitWord(x, 7, 11); // API
        if (w != -1) return w;
        if (x == 10) return 47;
        w = emitWord(x, 11, 12); // V1
        if (w != -1) return w;
        if (x == 13) return 47;
        int hc = emitHexCharFixed(x, 14, hash_u32(seed ^ 0x243f6a88u), 8);
        if (hc != -1) return hc;
        if (x == 22) return 32;
        if (x == 23) return 50;
        if (x == 24) return 48;
        if (x == 25) return 48;
        if (x == 26) return 32;
        w = emitWord(x, 27, 13); // OK
        if (w != -1) return w;
        if (x == 29) return 32;
        int ms = int(10u + (hash_u32(seed ^ 0x13198a2eu) % 990u));
        int dc = emitDecCharFixed(x, 30, ms, 3);
        if (dc != -1) return dc;
        w = emitWord(x, 33, 20); // MS
        if (w != -1) return w;
        return 32;
    }

    // Type 4: > SSH AAA.BBB.CCC.DDD:PPPP CONNECTED
    if (typ == 4) {
        int w = emitWord(x, 2, 14); // SSH
        if (w != -1) return w;
        if (x == 5) return 32;
        int ic = emitIpCharFixed(x, 6, seed);
        if (ic != -1) return ic;
        if (x == 21) return 58;
        int port = int(20u + (hash_u32(seed ^ 0x9e3779b9u) % 9950u));
        int dc = emitDecCharFixed(x, 22, port, 4);
        if (dc != -1) return dc;
        if (x == 26) return 32;
        w = emitWord(x, 27, 15); // CONNECTED
        if (w != -1) return w;
        return 32;
    }

    // Type 5: > SCAN AAA.BBB.CCC.DDD:PPPP OPEN
    if (typ == 5) {
        int w = emitWord(x, 2, 16); // SCAN
        if (w != -1) return w;
        if (x == 6) return 32;
        int ic = emitIpCharFixed(x, 7, seed);
        if (ic != -1) return ic;
        if (x == 22) return 58;
        int port = int(20u + (hash_u32(seed ^ 0x7f4a7c15u) % 9950u));
        int dc = emitDecCharFixed(x, 23, port, 4);
        if (dc != -1) return dc;
        if (x == 27) return 32;
        w = emitWord(x, 28, 17); // OPEN
        if (w != -1) return w;
        return 32;
    }

    // Type 6: > DECRYPT BLOCK NNNN ... OK
    if (typ == 6) {
        int w = emitWord(x, 2, 18); // DECRYPT
        if (w != -1) return w;
        if (x == 9) return 32;
        w = emitWord(x, 10, 19); // BLOCK
        if (w != -1) return w;
        if (x == 15) return 32;
        int blk = int(hash_u32(seed ^ 0xc2b2ae35u) % 10000u);
        int dc = emitDecCharFixed(x, 16, blk, 4);
        if (dc != -1) return dc;
        if (x == 20) return 32;
        if (x == 21) return 46;
        if (x == 22) return 46;
        if (x == 23) return 46;
        if (x == 24) return 32;
        w = emitWord(x, 25, 13); // OK
        if (w != -1) return w;
        return 32;
    }

    return 32;
}

vec3 patternTerminal(vec2 fragCoord, float t) {
    vec2 res = u_resolution;
    float dens = clamp((u_density - 0.85) / 0.45, 0.0, 1.0);

    float cellH = mix(22.0, 14.0, dens);
    float cellW = cellH * 0.62;
    int cols = int(max(floor(res.x / cellW), 1.0));
    int rows = int(max(floor(res.y / cellH), 1.0));

    float linesPerSec = mix(0.70, 1.60, dens);
    float lineF = t * linesPerSec;
    int currentLine = int(floor(lineF));
    float frac = fract(lineF);

    // Smooth upward scroll: shift the grid by a fraction of a line.
    float ySample = fragCoord.y - frac * cellH;
    float yCellF = ySample / cellH;
    int rowFromBottom = int(floor(yCellF));
    float yLocal = fract(yCellF);

    float xCellF = fragCoord.x / cellW;
    int col = int(floor(xCellF));
    float xLocal = fract(xCellF);

    vec3 bg = vec3(0.004, 0.006, 0.005);
    vec3 colOut = bg;

    if (col < 0 || col >= cols) {
        return colOut;
    }

    // Fade out near the top (older lines).
    float rowF = float(rowFromBottom);
    float fade = 1.0 - smoothstep(float(rows) - 2.0, float(rows) + 4.0, rowF);
    if (fade <= 0.0) {
        return colOut;
    }

    int lineId = currentLine - rowFromBottom;
    if (lineId < 0) {
        return colOut;
    }

    float age = t - float(lineId) / linesPerSec;
    float cps = mix(28.0, 52.0, dens);
    int typed = int(clamp(floor(age * cps), 0.0, float(cols)));

    uint seed = hash_u32(uint(lineId) * 747796405u + 2891336453u);

    int ch = 32;
    if (col < typed) {
        ch = termLineChar(lineId, col, cols, t, seed);
    } else if (col == typed) {
        // Cursor on the newest line only.
        float blink = step(0.5, fract(t * 1.6));
        if (rowFromBottom <= 0 && blink > 0.0) {
            ch = 95; // _
        }
    }

    // Glyph placement within the cell (slight padding).
    vec2 inCell = vec2(xLocal, 1.0 - yLocal);
    vec2 g = (inCell - vec2(0.10, 0.14)) / vec2(0.80, 0.74);
    float inside = step(0.0, g.x) * step(0.0, g.y) * step(g.x, 1.0) * step(g.y, 1.0);
    float m = termGlyph5x7(clamp(g, 0.0, 1.0), ch) * inside;

    // Phosphor-ish look: per-line tint + subtle glow/flicker.
    float flick = 0.92 + 0.08 * sin(t * 6.0 + float(col) * 0.7 + float(lineId) * 0.11);
    float grain = 0.97 + 0.06 * (hash21(fragCoord + vec2(t * 31.0, t * 17.0)) - 0.5);
    vec3 fg = vec3(0.10, 0.95, 0.20);
    vec3 hi = vec3(0.55, 1.0, 0.75);
    float hot = step(0.995, fract(u32_to_01(seed) + float(col) * 0.013));
    vec3 ink = mix(fg, hi, 0.35 * hot);

    float glow = 0.25 + 0.75 * smoothstep(0.0, 1.0, m);
    colOut += ink * (m * glow) * flick * grain * fade;

    // Background scanlines.
    float scan = 0.018 * sin((fragCoord.y + t * 45.0) * 6.28318 / 6.0);
    colOut *= 1.0 + scan;

    return colOut;
}

vec3 fractalPalette(float x) {
    // Smooth, colorful palette.
    vec3 a = vec3(0.50, 0.50, 0.50);
    vec3 b = vec3(0.50, 0.50, 0.50);
    vec3 c = vec3(1.00, 1.00, 1.00);
    vec3 d = vec3(0.00, 0.10, 0.20);
    return a + b * cos(6.28318 * (c * x + d));
}

vec3 fractalPaletteShift(float x, float shift) {
    return fractalPalette(x + shift);
}

vec2 cMul(vec2 a, vec2 b) {
    return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

vec3 fractalColor(vec2 p, int mode, vec2 juliaC, int maxIter, float paletteShift) {
    // mode=0 -> Mandelbrot, mode=1 -> Julia
    vec2 z = (mode == 0) ? vec2(0.0) : p;
    vec2 c = (mode == 0) ? p : juliaC;

    float m2 = dot(z, z);
    int i = 0;
    float trap = 1e9;
    for (int k = 0; k < 512; k++) {
        if (k >= maxIter) {
            break;
        }
        if (m2 > 16.0) {
            i = k;
            break;
        }
        z = cMul(z, z) + c;
        m2 = dot(z, z);
        trap = min(trap, abs(m2 - 0.12));
        i = k;
    }

    // Inside set -> orbit-trap shading (prevents "all black" when zooming deep).
    if (m2 <= 16.0 && i >= (maxIter - 1)) {
        float v = exp(-trap * 28.0);
        vec3 base = vec3(0.010, 0.012, 0.018);
        vec3 ink = fractalPaletteShift(0.30 + trap * 1.6 + 0.15 * float(mode), paletteShift);
        return mix(base, ink, 0.55 * v);
    }

    // Smooth iteration count.
    float mu = float(i);
    float l = log(max(sqrt(m2), 1e-6));
    float nu = log(max(l, 1e-6)) / log(2.0);
    float smoothIter = mu + 1.0 - nu;

    float x = smoothIter / float(maxIter);
    vec3 col = fractalPaletteShift(0.15 + 1.8 * x, paletteShift);
    col *= 0.35 + 0.90 * smoothstep(0.0, 1.0, x);
    return col;
}

vec3 patternFractals(vec2 fragCoord, float t) {
    vec2 res = u_resolution;
    float dens = clamp((u_density - 0.85) / 0.45, 0.0, 1.0);

    float segDur = 18.0;
    float u = t / segDur;
    float seg = floor(u);
    float f = fract(u);

    // Infinite loop: zoom-in then zoom-out.
    float zoomRange = mix(6.0, 11.0, dens);
    float zoomPhase = 0.5 - 0.5 * cos(6.28318 * f);
    float zoom = exp2(zoomPhase * zoomRange);
    float scale = 2.4 / zoom;

    // Smoothly switch targets near the end of each segment (when zoom is back near 1).
    float trans = smoothstep(0.84, 1.00, f);

    // Choose a target region deterministically per segment.
    uint s = uint(seg);
    uint baseSeed = uint(u_seed);
    uint h0 = hash_u32(baseSeed ^ (s * 1103515245u + 12345u));
    uint h1 = hash_u32(baseSeed ^ ((s + 1u) * 1103515245u + 12345u));
    int idx0 = int(h0 % 8u);
    int idx1 = int(h1 % 8u);
    int mode = int(h0 & 1u); // 0 Mandelbrot, 1 Julia
    float paletteShift =
        u32_to_01(hash_u32(baseSeed ^ (s * 2246822519u + 3266489917u))) * 1.5;
    vec2 centers[8] = vec2[8](
        vec2(-0.7436439, 0.1318259),
        vec2(-0.74529, 0.113075),
        vec2(-0.1010964, 0.9562865),
        vec2(-0.15652, 1.03225),
        vec2(-0.70176, -0.3842),
        vec2(0.0016437, 0.8224676),
        vec2(-1.25066, 0.02012),
        vec2(-0.835, 0.2321)
    );
    vec2 julias[8] = vec2[8](
        vec2(-0.8, 0.156),
        vec2(0.285, 0.01),
        vec2(-0.70176, -0.3842),
        vec2(0.37, 0.10),
        vec2(-0.4, 0.6),
        vec2(0.28, 0.53),
        vec2(-0.12, -0.77),
        vec2(0.355, 0.355)
    );

    // Mandelbrot should always be centered on the canonical (-0.5, 0) when zoom is low,
    // otherwise it looks "shifted" / squashed against one side.
    float pan = smoothstep(0.0, 0.30, zoomPhase);
    vec2 target = mix(centers[idx0], centers[idx1], trans);
    vec2 baseCenter = vec2(-0.5, 0.0);
    vec2 center = (mode == 0) ? mix(baseCenter, target, pan) : vec2(0.0);

    // Tiny drift only while zoomed-in (and only for Mandelbrot).
    float driftAmt = (0.012 / zoom) * pan * float(1 - mode);
    center += vec2(sin(t * 0.13 + float(idx0) * 2.1), cos(t * 0.11 + float(idx0) * 1.7)) * driftAmt;

    vec2 uv = (fragCoord - 0.5 * res) / res.y;
    float ang = 0.18 * sin(t * 0.06 + float(idx0) * 1.3);
    uv = mat2(cos(ang), -sin(ang), sin(ang), cos(ang)) * uv;

    vec2 p = center + uv * scale;

    // Occasionally use a fully random Julia constant (gives "new" fractals sometimes).
    float randomJulia = step(0.72, u32_to_01(hash_u32(h0 ^ 0xa5a5a5a5u)));
    vec2 jc0 = julias[idx0];
    vec2 jc1 = julias[idx1];
    vec2 jc = mix(jc0, jc1, trans);
    vec2 jr = vec2(
        -0.85 + 1.70 * u32_to_01(hash_u32(h0 ^ 0x1b873593u)),
        -0.85 + 1.70 * u32_to_01(hash_u32(h0 ^ 0x85ebca6bu))
    );
    jc = mix(jc, jr, randomJulia);
    jc += vec2(cos(t * 0.08), sin(t * 0.07)) * (0.015 + 0.01 * dens);

    int maxIter = int(mix(80.0, 210.0, dens));
    maxIter = int(clamp(float(maxIter) + (zoomRange * zoomPhase) * 7.0, 70.0, 360.0));

    vec3 col = fractalColor(p, mode, jc, maxIter, paletteShift);

    // Subtle background + vignette.
    vec2 vuv = fragCoord / res;
    float vign = smoothstep(1.10, 0.35, length(vuv - 0.5));
    vec3 bg = vec3(0.005, 0.006, 0.010);
    bg += vec3(0.02, 0.01, 0.03) * fbm(uv * 2.0 + t * 0.03);
    col = mix(bg, col, 0.85);
    col *= vign;

    return col;
}

vec3 patternLCARS(vec2 fragCoord, float t) {
    vec2 p = fragCoord;
    vec2 res = u_resolution;

    vec3 bg = vec3(0.006, 0.007, 0.010);
    vec3 col = bg;

    // LCARS-ish palette.
    vec3 c_orange = vec3(0.98, 0.58, 0.18);
    vec3 c_orange2 = vec3(0.97, 0.72, 0.32);
    vec3 c_tan = vec3(0.98, 0.80, 0.52);
    vec3 c_pink = vec3(0.90, 0.34, 0.62);
    vec3 c_purple = vec3(0.56, 0.43, 0.95);
    vec3 c_blue = vec3(0.20, 0.72, 0.92);
    vec3 c_cyan = vec3(0.22, 0.92, 0.84);
    vec3 c_red = vec3(0.92, 0.28, 0.24);
    vec3 c_text = vec3(0.05, 0.05, 0.06);

    float m = min(res.x, res.y);
    float pad = 0.03 * m;
    float leftW = 0.22 * res.x;
    float gap = 0.012 * m;
    float dens = clamp((u_density - 0.85) / 0.45, 0.0, 1.0);

    // Background polish: vignette + subtle grain.
    vec2 npos = (p - 0.5 * res) / res.y;
    float vign = smoothstep(1.2, 0.15, length(npos));
    col *= 0.55 + 0.45 * vign;
    col += (hash21(p + vec2(t * 17.0, t * 9.0)) - 0.5) * 0.010;

    // Left vertical stack (more "LCARS-like" with extensions + cutouts + labels).
    float top = pad;
    float bottom = res.y - pad;
    float usableH = bottom - top;
    int segs = 9 + int(floor(dens * 7.0 + 0.5));
    float segH = (usableH - float(segs - 1) * gap) / float(segs);
    for (int i = 0; i < 20; i++) {
        if (i >= segs) {
            break;
        }
        float y0 = top + float(i) * (segH + gap);
        float fi = float(i);
        float ext = step(0.55, hash11(19.13 * fi + 1.7));
        float extW = ext * (0.06 + 0.18 * hash11(3.77 * fi + 2.1)) * res.x;
        vec2 center = vec2(pad + leftW * 0.5 + extW * 0.5, y0 + segH * 0.5);
        vec2 hs = vec2(leftW * 0.5 + extW, segH * 0.5);
        float r = min(hs.x, hs.y) * 0.45;
        float dSeg = sdRoundRect(p - center, hs, r);
        float fill = aaFillFromSdf(dSeg);
        float glow = glowFromSdf(dSeg, 0.014 * m);

        vec3 cc = (i % 7 == 0) ? c_orange :
                  (i % 7 == 1) ? c_blue :
                  (i % 7 == 2) ? c_purple :
                  (i % 7 == 3) ? c_pink :
                  (i % 7 == 4) ? c_tan :
                  (i % 7 == 5) ? c_orange2 : c_cyan;

        float pulse = 0.92 + 0.08 * sin(t * (0.7 + 0.15 * fi) + fi * 1.6);
        float grad = 0.88 + 0.12 * ((p.y - y0) / max(segH, 1.0));
        vec3 fillCol = cc * pulse * grad;

        col = mix(col, fillCol, fill);
        col += fillCol * glow * 0.09;

        // Occasional right-side cutout (LCARS negative space).
        float doCut = step(0.55, hash11(7.31 * fi + 0.2));
        vec2 cutC = vec2(center.x + hs.x * (0.55 + 0.10 * hash11(2.7 * fi)), center.y);
        vec2 cutHS = vec2(hs.x * (0.16 + 0.08 * hash11(5.3 * fi)), hs.y * 0.34);
        float dCut = sdRoundRect(p - cutC, cutHS, cutHS.y * 0.7);
        float cut = aaFillFromSdf(dCut) * doCut;
        col = mix(col, bg, cut * fill);

        // Tiny "label lines" in the extension area.
        float label = ext * fill;
        for (int k = 0; k < 2; k++) {
            float fk = float(k);
            float ly = center.y + (fk - 0.5) * hs.y * 0.55;
            float lw = extW * (0.30 + 0.55 * hash11(fi * 9.2 + fk * 13.1));
            vec2 lC = vec2(center.x + hs.x - lw * 0.55, ly);
            vec2 lHS = vec2(lw * 0.5, hs.y * 0.10);
            float dL = sdRoundRect(p - lC, lHS, lHS.y);
            float l = aaFillFromSdf(dL) * label;
            col = mix(col, c_text, l * 0.75);
        }
    }

    // Bottom "elbow" block (classic LCARS frame piece).
    float elbowH = 0.13 * res.y;
    float elbowW = leftW + (0.30 + 0.05 * dens) * res.x;
    vec2 elbowC = vec2(pad + elbowW * 0.5, res.y - pad - elbowH * 0.5);
    vec2 elbowHS = vec2(elbowW * 0.5, elbowH * 0.5);
    float dElbow = sdRoundRect(p - elbowC, elbowHS, elbowH * 0.45);
    float elbowFill = aaFillFromSdf(dElbow);
    col = mix(col, c_orange, elbowFill);
    col += c_orange * glowFromSdf(dElbow, 0.018 * m) * 0.10;

    // Elbow cutouts + tiny buttons.
    vec2 eCutC = vec2(elbowC.x + elbowHS.x * 0.18, elbowC.y);
    vec2 eCutHS = vec2(elbowHS.x * 0.28, elbowHS.y * 0.40);
    float eCut = aaFillFromSdf(sdRoundRect(p - eCutC, eCutHS, eCutHS.y * 0.6));
    col = mix(col, bg, eCut * elbowFill);

    for (int b = 0; b < 4; b++) {
        float fb = float(b);
        vec2 bC = vec2(elbowC.x + elbowHS.x * (0.50 + 0.10 * fb), elbowC.y);
        vec2 bHS = vec2(elbowHS.x * 0.04, elbowHS.y * 0.18);
        float dB = sdRoundRect(p - bC, bHS, bHS.y);
        float bFill = aaFillFromSdf(dB) * elbowFill;
        vec3 bc = (b % 2 == 0) ? c_tan : c_orange2;
        col = mix(col, bc, bFill);
        col = mix(col, bg, aaFillFromSdf(sdRoundRect(p - (bC + vec2(0.0, -bHS.y * 0.15)),
                                                  bHS * vec2(0.55, 0.35), bHS.y * 0.35)) * bFill);
    }

    // Top bars (right side): segmented header + moving sweep.
    float topH = 0.14 * res.y;
    float rightX0 = leftW + pad + gap;
    float rightW = res.x - rightX0 - pad;

    vec2 h0C = vec2(rightX0 + rightW * 0.25, pad + topH * 0.5);
    vec2 h0HS = vec2(rightW * 0.25, topH * 0.5);
    float dH0 = sdRoundRect(p - h0C, h0HS, topH * 0.55);
    float h0 = aaFillFromSdf(dH0);
    col = mix(col, c_purple, h0);

    vec2 h1C = vec2(rightX0 + rightW * 0.68, pad + topH * 0.5);
    vec2 h1HS = vec2(rightW * 0.32, topH * 0.5);
    float dH1 = sdRoundRect(p - h1C, h1HS, topH * 0.55);
    float h1m = aaFillFromSdf(dH1);
    col = mix(col, c_orange2 * (0.92 + 0.08 * (p.x / res.x)), h1m);

    // Black inset + animated sweep inside the main header.
    vec2 hInsetC = h1C + vec2(-h1HS.x * 0.05, 0.0);
    vec2 hInsetHS = h1HS * vec2(0.72, 0.55);
    float dInset = sdRoundRect(p - hInsetC, hInsetHS, hInsetHS.y * 0.7);
    float inset = aaFillFromSdf(dInset);
    col = mix(col, bg, inset * h1m);

    float sweepX = rightX0 + rightW * (0.15 + 0.70 * fract(t * 0.06));
    float sweepW = rightW * 0.10;
    float sweep = smoothstep(sweepW, 0.0, abs(p.x - sweepX)) * inset * h1m;
    col = mix(col, c_orange * 1.1, sweep * 0.55);

    vec2 h2C = vec2(rightX0 + rightW * 0.94, pad + topH * 0.5);
    vec2 h2HS = vec2(rightW * 0.08, topH * 0.5);
    float dH2 = sdRoundRect(p - h2C, h2HS, topH * 0.55);
    float h2m = aaFillFromSdf(dH2);
    col = mix(col, c_pink, h2m);

    // Right panels (two large blocks).
    float y1 = pad + topH + gap;
    float hMain = (res.y - y1 - pad - gap - elbowH) * 0.58;
    float y2 = y1 + hMain + gap;
    float hBottom = res.y - y2 - pad - elbowH;
    vec2 p1c = vec2(rightX0 + rightW * 0.5, y1 + hMain * 0.5);
    vec2 p2c = vec2(rightX0 + rightW * 0.5, y2 + hBottom * 0.5);

    vec2 p1hs = vec2(rightW * 0.5, hMain * 0.5);
    vec2 p2hs = vec2(rightW * 0.5, hBottom * 0.5);

    float dP1 = sdRoundRect(p - p1c, p1hs, 0.035 * m);
    float dP2 = sdRoundRect(p - p2c, p2hs, 0.035 * m);
    float p1 = aaFillFromSdf(dP1);
    float p2 = aaFillFromSdf(dP2);
    col = mix(col, c_blue * (0.12 + 0.10 * vign), p1);
    col = mix(col, c_purple * (0.11 + 0.10 * vign), p2);
    col += c_blue * glowFromSdf(dP1, 0.02 * m) * 0.05;
    col += c_purple * glowFromSdf(dP2, 0.02 * m) * 0.05;

    // Panel outlines + internal dividers.
    float out1 = aaStrokeFromSdf(dP1, 1.0);
    float out2 = aaStrokeFromSdf(dP2, 1.0);
    col = mix(col, vec3(0.02, 0.02, 0.03), out1 * 0.75);
    col = mix(col, vec3(0.02, 0.02, 0.03), out2 * 0.75);

    // Top panel: grid + waveform + tiny "buttons".
    vec2 p1min = p1c - p1hs;
    vec2 p1max = p1c + p1hs;
    vec2 q1 = (p - p1min) / max(p1max - p1min, vec2(1.0));
    float inside1 = step(0.0, q1.x) * step(0.0, q1.y) * step(q1.x, 1.0) * step(q1.y, 1.0) * p1;

    float grid = (smoothstep(0.995, 1.0, fract(q1.x * 22.0)) + smoothstep(0.995, 1.0, fract(q1.y * 14.0))) * 0.35;
    col += c_cyan * grid * inside1 * 0.08;

    float waveY = 0.22 + 0.10 * sin((q1.x * (8.0 + 6.0 * dens) + t * 0.18) * 6.28318);
    float wave = smoothstep(0.010, 0.0, abs(q1.y - waveY));
    col = mix(col, c_cyan, wave * inside1 * 0.75);

    for (int bb = 0; bb < 6; bb++) {
        float fbb = float(bb);
        vec2 bC = vec2(p1min.x + (0.10 + 0.12 * fbb) * (p1max.x - p1min.x),
                      p1min.y + 0.12 * (p1max.y - p1min.y));
        vec2 bHS = vec2(0.040, 0.030) * vec2(p1max.x - p1min.x, p1max.y - p1min.y);
        float d = sdRoundRect(p - bC, bHS, bHS.y);
        float bm = aaFillFromSdf(d) * inside1;
        vec3 bc = (bb % 3 == 0) ? c_orange : ((bb % 3 == 1) ? c_tan : c_pink);
        col = mix(col, bc, bm);
        col = mix(col, bg, aaFillFromSdf(sdRoundRect(p - bC, bHS * vec2(0.55, 0.35), bHS.y * 0.35)) * bm);
    }

    // Bottom panel: gauge + ticker.
    vec2 p2min = p2c - p2hs;
    vec2 p2max = p2c + p2hs;
    vec2 q2 = (p - p2min) / max(p2max - p2min, vec2(1.0));
    float inside2 = step(0.0, q2.x) * step(0.0, q2.y) * step(q2.x, 1.0) * step(q2.y, 1.0) * p2;

    vec2 gC = vec2(p2max.x - 0.20 * (p2max.x - p2min.x), p2min.y + 0.34 * (p2max.y - p2min.y));
    float r0 = 0.07 * m;
    float r1 = 0.10 * m;
    float a0 = -1.6;
    float a1 = 1.1 + 0.8 * sin(t * 0.25);
    float gauge = aaRingSector(p, gC, r0, r1, a0, a1) * inside2;
    col = mix(col, c_orange2, gauge);

    for (int tick = 0; tick < 10; tick++) {
        float ft = float(tick);
        float a = mix(-1.6, 1.1, ft / 9.0);
        vec2 pa = gC + vec2(cos(a), sin(a)) * (r0 * 0.92);
        vec2 pb = gC + vec2(cos(a), sin(a)) * (r1 * 1.06);
        float tmask = aaStrokeSegment(p, pa, pb, 1.2) * inside2;
        col = mix(col, c_tan, tmask * 0.65);
    }

    // Scrolling ticker blocks.
    float tickerH = 0.09 * (p2max.y - p2min.y);
    vec2 tC = vec2(p2min.x + 0.48 * (p2max.x - p2min.x), p2max.y - tickerH * 0.65);
    vec2 tHS = vec2(0.46 * (p2max.x - p2min.x), tickerH * 0.5);
    float dT = sdRoundRect(p - tC, tHS, tHS.y);
    float tFill = aaFillFromSdf(dT) * inside2;
    col = mix(col, c_orange * 0.35, tFill);
    float scroll = fract(t * 0.08);
    float blocks = smoothstep(0.0, 0.02, sin((q2.x * 16.0 + scroll * 6.28318) * 6.28318));
    col = mix(col, c_orange, blocks * tFill * 0.20);

    // Alarm/indicator (blinking).
    float blink = step(0.5, fract(t * 1.6));
    float dot = aaFillCircle(p, vec2(rightX0 + rightW * 0.93, y1 + hMain * 0.12), 0.0085 * m);
    col += c_red * dot * (0.35 + 0.65 * blink);

    // Subtle scanlines.
    float scan = 0.015 * sin((p.y + t * 40.0) * 6.28318 / 6.0);
    col *= 1.0 + scan;

    return col;
}

void main() {
    vec2 fc = gl_FragCoord.xy;
    float t = u_time;

    if (u_pass == 1) {
        if (u_pattern == 19) {
            // Gray-Scott reaction-diffusion simulation step.
            vec2 uv = (fc + vec2(0.5)) / u_resolution;
            vec2 px = 1.0 / max(u_resolution, vec2(1.0));

            vec2 c = texture(u_state, uv).rg;
            vec2 n = texture(u_state, uv + vec2(0.0, px.y)).rg;
            vec2 s = texture(u_state, uv - vec2(0.0, px.y)).rg;
            vec2 e = texture(u_state, uv + vec2(px.x, 0.0)).rg;
            vec2 w = texture(u_state, uv - vec2(px.x, 0.0)).rg;
            vec2 ne = texture(u_state, uv + vec2(px.x, px.y)).rg;
            vec2 nw = texture(u_state, uv + vec2(-px.x, px.y)).rg;
            vec2 se = texture(u_state, uv + vec2(px.x, -px.y)).rg;
            vec2 sw = texture(u_state, uv + vec2(-px.x, -px.y)).rg;

            vec2 lap = (n + s + e + w) * 0.20 + (ne + nw + se + sw) * 0.05 - c;

            uint base = hash_u32(uint(u_seed) ^ 0x51633e2du);
            float r1 = u32_to_01(base);
            float r2 = u32_to_01(hash_u32(base ^ 0x68bc21ebu));
            float r3 = u32_to_01(hash_u32(base ^ 0x02e5be93u));

            float dens = clamp((u_density - 0.85) / 0.45, 0.0, 1.0);
            float Du = mix(0.14, 0.18, r3);
            float Dv = mix(0.06, 0.10, 1.0 - r3);
            float F = mix(0.032, 0.055, r1);
            float K = mix(0.056, 0.072, r2);
            // Slow modulation to avoid settling into a static equilibrium.
            float mod1 = sin(t * 0.05 + r1 * 6.28318) * 0.003;
            float mod2 = cos(t * 0.04 + r2 * 6.28318) * 0.003;
            F = clamp(F + mod1, 0.020, 0.080);
            K = clamp(K + mod2, 0.040, 0.090);

            float U = c.r;
            float V = c.g;
            float UVV = U * V * V;

            float dU = Du * lap.r - UVV + F * (1.0 - U);
            float dV = Dv * lap.g + UVV - (F + K) * V;

            float dt = 1.0;
            U = clamp(U + dU * dt, 0.0, 1.0);
            V = clamp(V + dV * dt, 0.0, 1.0);

            // Keep the system alive: mouse injection + occasional random "droplets".
            vec2 muv = u_mouse / max(u_resolution, vec2(1.0));
            if (u_mouse_active == 1) {
                vec2 d = uv - muv;
                float m = exp(-dot(d, d) / (2.0 * 0.0009));
                V = clamp(V + 0.55 * m, 0.0, 1.0);
                U = clamp(U - 0.35 * m, 0.0, 1.0);
            }

            float event = floor(t * 0.22); // ~every 4.5s
            float pf = fract(t * 0.22);
            float pulse = smoothstep(0.0, 0.06, pf) * (1.0 - smoothstep(0.16, 0.26, pf));
            uint eh = hash_u32(uint(u_seed) ^ uint(event) * 2246822519u);
            vec2 cpos = vec2(u32_to_01(eh), u32_to_01(hash_u32(eh ^ 0x9e3779b9u)));
            vec2 d2 = uv - cpos;
            float rr = mix(0.010, 0.030, u32_to_01(hash_u32(eh ^ 0x85ebca6bu)));
            float inj = pulse * exp(-dot(d2, d2) / (2.0 * rr * rr));
            V = clamp(V + 0.40 * inj, 0.0, 1.0);
            U = clamp(U - 0.24 * inj, 0.0, 1.0);

            fragColor = vec4(U, V, 0.0, 1.0);
        } else {
            fragColor = vec4(0.0, 0.0, 0.0, 1.0);
        }
        return;
    }

    vec3 col;
    if (u_pattern == 0) col = patternMatrix(fc, t);
    else if (u_pattern == 1) col = patternStars(fc, t);
    else if (u_pattern == 2) col = patternGeometry(fc, t);
    else if (u_pattern == 3) col = patternFlowfield(fc, t);
    else if (u_pattern == 4) col = patternAurora(fc, t);
    else if (u_pattern == 5) col = patternPlasma(fc, t);
    else if (u_pattern == 6) col = patternBokeh(fc, t);
    else if (u_pattern == 7) col = patternConstellation(fc, t);
    else if (u_pattern == 8) col = patternLissajous(fc, t);
    else if (u_pattern == 9) col = patternWaves(fc, t);
    else if (u_pattern == 10) col = patternVoronoi(fc, t);
    else if (u_pattern == 11) col = patternScanline(fc, t);
    else if (u_pattern == 12) col = patternFireflies(fc, t);
    else if (u_pattern == 13) col = patternSmokeInk(fc, t);
    else if (u_pattern == 14) col = patternWaterRipples(fc, t);
    else if (u_pattern == 15) col = patternMatrixRain3D(fc, t);
    else if (u_pattern == 16) col = patternLCARS(fc, t);
    else if (u_pattern == 17) col = patternTerminal(fc, t);
    else if (u_pattern == 18) col = patternFractals(fc, t);
    else if (u_pattern == 19) {
        vec2 uv = fc / u_resolution;
        vec2 px = 1.0 / max(u_state_size, vec2(1.0));
        vec2 st = texture(u_state, uv).rg;
        vec2 stn = texture(u_state, uv + vec2(0.0, px.y)).rg;
        vec2 ste = texture(u_state, uv + vec2(px.x, 0.0)).rg;
        vec2 sts = texture(u_state, uv - vec2(0.0, px.y)).rg;
        vec2 stw = texture(u_state, uv - vec2(px.x, 0.0)).rg;
        vec2 st_smooth = (st * 0.50 + (stn + ste + sts + stw) * 0.125);

        float U = st_smooth.r;
        float V = st_smooth.g;
        float v = V;

        float edge = length(vec2(dFdx(v), dFdy(v)));
        edge = clamp(edge * 120.0, 0.0, 1.0);

        uint base = hash_u32(uint(u_seed) ^ 0x2c1b3c6du);
        float pal = u32_to_01(base);
        vec3 a = vec3(0.02, 0.03, 0.04);
        vec3 b = vec3(0.10, 0.95, 0.35);
        vec3 c0 = vec3(0.20, 0.55, 0.95);
        vec3 d0 = vec3(0.95, 0.45, 0.20);
        vec3 fg = mix(b, c0, pal);
        vec3 hi = mix(c0, d0, pal);

        float mask = smoothstep(0.10, 0.65, v);
        col = mix(a, fg, mask);
        col = mix(col, hi, edge * 0.85);
        col *= 0.65 + 0.35 * smoothstep(0.0, 1.0, clamp(U - V, 0.0, 1.0));

        // Subtle vignette.
        vec2 vuv = uv - 0.5;
        col *= smoothstep(0.95, 0.25, length(vuv));
    } else col = vec3(0.0);

    col = applyTheme(col);
    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
"#;
