use crate::config::{PatternDensity, PatternTheme};
use directories::ProjectDirs;
use glow::HasContext;
use gtk4::prelude::*;
use gtk4::{EventControllerMotion, GLArea};
use serde_json::json;
use std::cell::RefCell;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PYTHON_HOST: &str = include_str!("python_plugin_host.py");

#[derive(Clone)]
struct PythonSharedState {
    width: Arc<AtomicU32>,
    height: Arc<AtomicU32>,
    mouse_x_bits: Arc<AtomicU32>,
    mouse_y_bits: Arc<AtomicU32>,
    mouse_active: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl PythonSharedState {
    fn new() -> Self {
        Self {
            width: Arc::new(AtomicU32::new(0)),
            height: Arc::new(AtomicU32::new(0)),
            mouse_x_bits: Arc::new(AtomicU32::new(0f32.to_bits())),
            mouse_y_bits: Arc::new(AtomicU32::new(0f32.to_bits())),
            mouse_active: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Default)]
struct LatestFrame {
    generation: u64,
    width: i32,
    height: i32,
    rgba: Vec<u8>,
}

struct PythonGlResources {
    _libgl: libloading::Library,
    gl: glow::Context,
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    tex: glow::NativeTexture,
    u_tex: Option<glow::NativeUniformLocation>,
    uploaded_generation: u64,
    tex_size: (i32, i32),
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    latest: Arc<Mutex<LatestFrame>>,
}

pub fn build_python_script_area(
    script_path: &str,
    density: PatternDensity,
    theme: PatternTheme,
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

    let script_path = script_path.trim().to_string();
    let shared = PythonSharedState::new();
    let resources: Rc<RefCell<Option<PythonGlResources>>> = Rc::new(RefCell::new(None));

    {
        let shared_for_motion = shared.clone();
        let gl_area_for_scale = gl_area.clone();
        let controller = EventControllerMotion::new();
        controller.connect_motion(move |_, x, y| {
            let scale = gl_area_for_scale.scale_factor().max(1) as f32;
            shared_for_motion
                .mouse_x_bits
                .store((x as f32 * scale).to_bits(), Ordering::Relaxed);
            shared_for_motion
                .mouse_y_bits
                .store((y as f32 * scale).to_bits(), Ordering::Relaxed);
            shared_for_motion
                .mouse_active
                .store(true, Ordering::Relaxed);
        });
        gl_area.add_controller(controller);
    }

    {
        gl_area.add_tick_callback(|widget, _frame_clock| {
            // Drive the GLArea render loop.
            widget.queue_render();
            glib::ControlFlow::Continue
        });
    }

    {
        let resources = resources.clone();
        let shared = shared.clone();
        let script_path = script_path.clone();
        gl_area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() {
                return;
            }

            let libgl = match unsafe { libloading::Library::new("libGL.so.1") } {
                Ok(lib) => lib,
                Err(err) => {
                    eprintln!("Python GL init failed: unable to open libGL.so.1: {err}");
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

            let (program, u_tex) = match unsafe { compile_program(&gl) } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("Python GL init failed: {err}");
                    return;
                }
            };

            let vao = match unsafe { gl.create_vertex_array() } {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("Python GL init failed: create_vertex_array: {err}");
                    return;
                }
            };

            let tex = match unsafe { gl.create_texture() } {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Python GL init failed: create_texture: {err}");
                    return;
                }
            };

            unsafe {
                gl.bind_vertex_array(Some(vao));
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
                gl.bind_vertex_array(None);
            }

            let latest: Arc<Mutex<LatestFrame>> = Arc::new(Mutex::new(LatestFrame::default()));
            let stop = shared.stop.clone();

            let worker = {
                let latest = latest.clone();
                let shared = shared.clone();
                let script_path = script_path.clone();
                thread::spawn(move || {
                    run_python_worker(script_path, shared, latest, density, theme);
                })
            };

            *resources.borrow_mut() = Some(PythonGlResources {
                _libgl: libgl,
                gl,
                program,
                vao,
                tex,
                u_tex,
                uploaded_generation: 0,
                tex_size: (0, 0),
                stop,
                worker: Some(worker),
                latest,
            });
        });
    }

    {
        let resources = resources.clone();
        gl_area.connect_unrealize(move |area| {
            if let Some(mut res) = resources.borrow_mut().take() {
                res.stop.store(true, Ordering::Relaxed);
                if let Some(worker) = res.worker.take() {
                    thread::spawn(move || {
                        let _ = worker.join();
                    });
                }

                area.make_current();
                unsafe {
                    res.gl.delete_texture(res.tex);
                    res.gl.delete_vertex_array(res.vao);
                    res.gl.delete_program(res.program);
                }
            }
        });
    }

    {
        let resources = resources.clone();
        let shared = shared.clone();
        gl_area.connect_render(move |area, _ctx| {
            let mut binding = resources.borrow_mut();
            let Some(res) = binding.as_mut() else {
                return glib::Propagation::Stop;
            };

            let scale = area.scale_factor().max(1) as f32;
            let target_w = (area.width() as f32 * scale).max(1.0);
            let target_h = (area.height() as f32 * scale).max(1.0);
            let q = python_quality_scale(density);

            let w = (target_w * q).round().clamp(16.0, 8192.0) as u32;
            let h = (target_h * q).round().clamp(16.0, 8192.0) as u32;
            shared.width.store(w, Ordering::Relaxed);
            shared.height.store(h, Ordering::Relaxed);

            // Upload latest frame if needed.
            {
                let latest = res.latest.lock().unwrap();
                if latest.generation != res.uploaded_generation && !latest.rgba.is_empty() {
                    unsafe {
                        res.gl.active_texture(glow::TEXTURE0);
                        res.gl.bind_texture(glow::TEXTURE_2D, Some(res.tex));
                        res.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                        if res.tex_size != (latest.width, latest.height) {
                            res.gl.tex_image_2d(
                                glow::TEXTURE_2D,
                                0,
                                glow::RGBA8 as i32,
                                latest.width,
                                latest.height,
                                0,
                                glow::RGBA,
                                glow::UNSIGNED_BYTE,
                                Some(&latest.rgba),
                            );
                            res.tex_size = (latest.width, latest.height);
                        } else {
                            res.gl.tex_sub_image_2d(
                                glow::TEXTURE_2D,
                                0,
                                0,
                                0,
                                latest.width,
                                latest.height,
                                glow::RGBA,
                                glow::UNSIGNED_BYTE,
                                glow::PixelUnpackData::Slice(&latest.rgba),
                            );
                        }
                        res.gl.bind_texture(glow::TEXTURE_2D, None);
                    }
                    res.uploaded_generation = latest.generation;
                }
            }

            unsafe {
                res.gl.viewport(0, 0, target_w as i32, target_h as i32);
                res.gl.disable(glow::DEPTH_TEST);
                res.gl.disable(glow::STENCIL_TEST);
                res.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                res.gl.clear(glow::COLOR_BUFFER_BIT);

                res.gl.use_program(Some(res.program));
                res.gl.bind_vertex_array(Some(res.vao));
                res.gl.active_texture(glow::TEXTURE0);
                res.gl.bind_texture(glow::TEXTURE_2D, Some(res.tex));
                if let Some(loc) = &res.u_tex {
                    res.gl.uniform_1_i32(Some(loc), 0);
                }
                res.gl.draw_arrays(glow::TRIANGLES, 0, 3);
                res.gl.bind_texture(glow::TEXTURE_2D, None);
                res.gl.bind_vertex_array(None);
                res.gl.use_program(None);
            }

            // Ensure animation continues even if Python is slow.
            area.queue_render();
            glib::Propagation::Stop
        });
    }

    gl_area
}

fn python_quality_scale(density: PatternDensity) -> f32 {
    match density {
        PatternDensity::Low => 0.5,
        PatternDensity::Medium => 0.75,
        PatternDensity::High => 1.0,
    }
}

fn python_host_cache_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "vesper") {
        proj_dirs.cache_dir().join("python_plugin_host.py")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join(".cache")
            .join("vesper")
            .join("python_plugin_host.py")
    }
}

fn ensure_python_host_script() -> Result<PathBuf, String> {
    let host_path = python_host_cache_path();
    if let Some(parent) = host_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return Err(format!("failed to create cache dir: {err}"));
        }
    }
    let needs_write = match fs::read_to_string(&host_path) {
        Ok(existing) => existing != PYTHON_HOST,
        Err(_) => true,
    };
    if needs_write {
        if let Err(err) = fs::write(&host_path, PYTHON_HOST) {
            return Err(format!("failed to write python host script: {err}"));
        }
    }
    Ok(host_path)
}

fn try_spawn_python(host: &Path, script: &Path) -> Result<(Child, ChildStdin, ChildStdout), String> {
    for python in ["python3", "python"] {
        let mut cmd = Command::new(python);
        cmd.arg("-u").arg(host).arg(script);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        match cmd.spawn() {
            Ok(mut child) => {
                let stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| "python stdin unavailable".to_string())?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| "python stdout unavailable".to_string())?;
                return Ok((child, stdin, stdout));
            }
            Err(err) => {
                eprintln!("Python plugin: failed to start {python}: {err}");
            }
        }
    }
    Err("python not found (tried: python3, python)".to_string())
}

fn run_python_worker(
    script_path: String,
    shared: PythonSharedState,
    latest: Arc<Mutex<LatestFrame>>,
    density: PatternDensity,
    theme: PatternTheme,
) {
    let host_path = match ensure_python_host_script() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Python plugin: {err}");
            return;
        }
    };
    let script_path = PathBuf::from(script_path);
    if !script_path.is_file() {
        eprintln!("Python plugin: script not found: {}", script_path.display());
        return;
    }

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u32;
    let mut frame_index: u64 = 0;

    let mut child: Option<Child> = None;
    let mut stdin: Option<ChildStdin> = None;
    let mut stdout: Option<ChildStdout> = None;

    let start = Instant::now();
    let mut last_t = 0.0f32;

    while !shared.stop.load(Ordering::Relaxed) {
        let loop_start = Instant::now();
        if child.is_none() {
            match try_spawn_python(&host_path, &script_path) {
                Ok((c, si, so)) => {
                    child = Some(c);
                    stdin = Some(si);
                    stdout = Some(so);
                }
                Err(err) => {
                    eprintln!("Python plugin: {err}");
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }
            }
        }

        let w = shared.width.load(Ordering::Relaxed) as i32;
        let h = shared.height.load(Ordering::Relaxed) as i32;
        if w <= 0 || h <= 0 {
            thread::sleep(Duration::from_millis(50));
            continue;
        }

        let mx = f32::from_bits(shared.mouse_x_bits.load(Ordering::Relaxed));
        let my = f32::from_bits(shared.mouse_y_bits.load(Ordering::Relaxed));
        let md = shared.mouse_active.swap(false, Ordering::Relaxed);

        let t = start.elapsed().as_secs_f32();
        let dt = (t - last_t).max(0.0);
        last_t = t;

        let msg = json!({
            "cmd": "frame",
            "w": w,
            "h": h,
            "t": t,
            "dt": dt,
            "frame": frame_index,
            "seed": seed,
            "mx": mx,
            "my": my,
            "md": md,
            "theme": theme_id(theme),
            "q": python_quality_scale(density),
        })
        .to_string();

        let mut should_restart = false;

        if let Some(si) = stdin.as_mut() {
            if let Err(err) = si.write_all(msg.as_bytes()).and_then(|_| si.write_all(b"\n")) {
                eprintln!("Python plugin: write failed: {err}");
                should_restart = true;
            } else if let Err(err) = si.flush() {
                eprintln!("Python plugin: flush failed: {err}");
                should_restart = true;
            }
        }

        if !should_restart {
            if let Some(so) = stdout.as_mut() {
                match read_python_message(so) {
                    Ok(PythonMessage::Frame(bytes)) => {
                        if bytes.len() != (w as usize * h as usize * 4) {
                            eprintln!(
                                "Python plugin: invalid frame size: got {}, expected {}",
                                bytes.len(),
                                w as usize * h as usize * 4
                            );
                        } else {
                            let mut lf = latest.lock().unwrap();
                            lf.width = w;
                            lf.height = h;
                            lf.rgba = bytes;
                            lf.generation = lf.generation.wrapping_add(1);
                        }
                    }
                    Ok(PythonMessage::Error(msg)) => {
                        eprintln!("Python plugin error:\n{msg}");
                    }
                    Err(err) => {
                        eprintln!("Python plugin: read failed: {err}");
                        should_restart = true;
                    }
                }
            }
        }

        frame_index = frame_index.wrapping_add(1);

        if should_restart {
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            stdin = None;
            stdout = None;
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        // Avoid busy-looping if Python is very fast.
        let target_frame = Duration::from_millis(16);
        let elapsed = loop_start.elapsed();
        if elapsed < target_frame {
            thread::sleep(target_frame - elapsed);
        }
    }

    if let Some(mut si) = stdin.take() {
        let _ = si.write_all(b"{\"cmd\":\"quit\"}\n");
        let _ = si.flush();
    }
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

enum PythonMessage {
    Frame(Vec<u8>),
    Error(String),
}

fn read_python_message(stdout: &mut ChildStdout) -> Result<PythonMessage, String> {
    let mut header = [0u8; 8];
    stdout
        .read_exact(&mut header)
        .map_err(|e| format!("read header: {e}"))?;
    let magic = &header[0..4];
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let mut payload = vec![0u8; len];
    if len > 0 {
        stdout
            .read_exact(&mut payload)
            .map_err(|e| format!("read payload: {e}"))?;
    }
    if magic == b"RSFR" {
        Ok(PythonMessage::Frame(payload))
    } else if magic == b"RSER" {
        Ok(PythonMessage::Error(
            String::from_utf8_lossy(&payload).to_string(),
        ))
    } else {
        Err(format!(
            "unknown python message: {:?} ({} bytes)",
            magic, len
        ))
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

unsafe fn compile_program(
    gl: &glow::Context,
) -> Result<(glow::NativeProgram, Option<glow::NativeUniformLocation>), String> {
    let vs_src = r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 pos;
    if (gl_VertexID == 0) pos = vec2(-1.0, -1.0);
    else if (gl_VertexID == 1) pos = vec2(3.0, -1.0);
    else pos = vec2(-1.0, 3.0);
    gl_Position = vec4(pos, 0.0, 1.0);
    v_uv = 0.5 * (pos + 1.0);
}
"#;

    let fs_src = r#"#version 330 core
in vec2 v_uv;
out vec4 o_color;
uniform sampler2D u_tex;
void main() {
    o_color = texture(u_tex, v_uv);
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
