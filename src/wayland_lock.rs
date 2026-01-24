use crate::config::{ScreensaverMode, SettingsProfile};
use crate::AppMessage;
use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};

pub struct SessionLockController {
    stop_fd: RawFd,
}

impl SessionLockController {
    pub fn stop(&self) {
        let buf = [b'S'];
        let _ = unsafe { libc::write(self.stop_fd, buf.as_ptr() as *const _, 1) };
    }

    pub fn set_battery_black(&self, enabled: bool) {
        let buf = [if enabled { b'B' } else { b'D' }];
        let _ = unsafe { libc::write(self.stop_fd, buf.as_ptr() as *const _, 1) };
    }
}

impl Drop for SessionLockController {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::close(self.stop_fd);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Color {
    fn argb(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

#[derive(Clone)]
enum Background {
    Solid(Color),
    Gradient { start: Color, end: Color },
}

impl Background {
    fn from_mode(mode: &ScreensaverMode) -> Self {
        match mode {
            ScreensaverMode::Color(hex) => parse_hex_color(hex)
                .map(Background::Solid)
                .unwrap_or_else(|| Background::Solid(Color::BLACK)),
            ScreensaverMode::Gradient { start, end } => {
                let start = parse_hex_color(start).unwrap_or(Color::BLACK);
                let end = parse_hex_color(end).unwrap_or(Color::BLACK);
                Background::Gradient { start, end }
            }
            _ => Background::Solid(Color::BLACK),
        }
    }

    fn fill(&self, pixels: &mut [u32], width: u32, height: u32) {
        match self {
            Background::Solid(c) => pixels.fill(c.argb()),
            Background::Gradient { start, end } => {
                let w = width.max(1) as f32;
                let h = height.max(1) as f32;
                for y in 0..height {
                    for x in 0..width {
                        let t = ((x as f32) / w + (y as f32) / h) * 0.5;
                        let t = t.clamp(0.0, 1.0);
                        let lerp = |a: u8, b: u8| -> u8 {
                            (a as f32 + (b as f32 - a as f32) * t).round() as u8
                        };
                        let c = Color {
                            r: lerp(start.r, end.r),
                            g: lerp(start.g, end.g),
                            b: lerp(start.b, end.b),
                            a: 0xFF,
                        };
                        pixels[(y * width + x) as usize] = c.argb();
                    }
                }
            }
        }
    }
}

impl Color {
    const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0xFF,
    };
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let s = hex.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color { r, g, b, a: 0xFF })
}

struct ShmBuffer {
    file: File,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    map: *mut u8,
    len: usize,
    width: u32,
    height: u32,
}

impl ShmBuffer {
    fn new<StateT>(
        shm: &wl_shm::WlShm,
        width: u32,
        height: u32,
        qh: &QueueHandle<StateT>,
    ) -> Result<Self, String>
    where
        StateT: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
    {
        let stride = width
            .checked_mul(4)
            .ok_or_else(|| "width overflow".to_string())?;
        let len = stride
            .checked_mul(height)
            .ok_or_else(|| "buffer size overflow".to_string())? as usize;

        let fd = create_memfd("vesper-lock")
            .or_else(|_| create_tmpfile("/tmp/vesper-lock-XXXXXX"))?;
        let raw_fd = fd.into_raw_fd();
        let file = unsafe { File::from_raw_fd(raw_fd) };
        set_file_len(&file, len as u64)?;

        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err("mmap failed".to_string());
        }

        let pool = shm.create_pool(file.as_fd(), len as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        Ok(Self {
            file,
            pool,
            buffer,
            map: map as *mut u8,
            len,
            width,
            height,
        })
    }

    fn pixels_mut(&mut self) -> &mut [u32] {
        let bytes = unsafe { std::slice::from_raw_parts_mut(self.map, self.len) };
        bytemuck::cast_slice_mut(bytes)
    }
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::munmap(self.map as *mut _, self.len);
        }
        self.buffer.destroy();
        self.pool.destroy();
        // file closes automatically
    }
}

fn set_file_len(file: &File, len: u64) -> Result<(), String> {
    let r = unsafe { libc::ftruncate(file.as_raw_fd(), len as libc::off_t) };
    if r != 0 {
        return Err("ftruncate failed".to_string());
    }
    Ok(())
}

fn create_memfd(name: &str) -> Result<OwnedFd, String> {
    let cname = CString::new(name).map_err(|_| "bad memfd name".to_string())?;
    let fd = unsafe { libc::memfd_create(cname.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err("memfd_create failed".to_string());
    }
    // Safety: fd is owned by us and valid.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn create_tmpfile(template: &str) -> Result<OwnedFd, String> {
    let mut bytes = CString::new(template)
        .map_err(|_| "bad template".to_string())?
        .into_bytes();
    bytes.push(0);
    let ptr = bytes.as_mut_ptr() as *mut libc::c_char;
    let fd = unsafe { libc::mkstemp(ptr) };
    if fd < 0 {
        return Err("mkstemp failed".to_string());
    }
    unsafe {
        libc::unlink(ptr);
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn create_pipe() -> Result<(RawFd, RawFd), String> {
    let mut fds = [0; 2];
    let r = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) };
    if r != 0 {
        return Err("pipe2 failed".to_string());
    }
    Ok((fds[0], fds[1]))
}

struct LockSurfaceState {
    surface: wl_surface::WlSurface,
    lock_surface: ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
    shm_buffer: Option<ShmBuffer>,
    last_serial: Option<u32>,
}

struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    seat: Option<wl_seat::WlSeat>,
    outputs: Vec<wl_output::WlOutput>,
    lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    lock: Option<ext_session_lock_v1::ExtSessionLockV1>,
    lock_surfaces: Vec<LockSurfaceState>,
    default_background: Background,
    background: Background,
    locked: bool,
    ready_tx: Option<mpsc::Sender<bool>>,
    activity_sent: bool,
    sender: mpsc::Sender<AppMessage>,
    started_at: Option<Instant>,
    mouse_wake_delay_ms: u64,
    last_pointer_pos: Option<(f64, f64)>,
    stop_read_fd: RawFd,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        match interface.as_str() {
            "wl_compositor" => {
                let v = version.min(4);
                state.compositor = Some(registry.bind(name, v, qh, ()));
            }
            "wl_shm" => {
                state.shm = Some(registry.bind(name, 1, qh, ()));
            }
            "wl_seat" => {
                let v = version.min(5);
                state.seat = Some(registry.bind(name, v, qh, ()));
            }
            "wl_output" => {
                let v = version.min(2);
                state.outputs.push(registry.bind(name, v, qh, ()));
            }
            "ext_session_lock_manager_v1" => {
                state.lock_manager = Some(registry.bind(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let WEnum::Value(capabilities) = capabilities else {
                return;
            };
            if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wl_keyboard::Event::Key {
            state: key_state, ..
        } = event
        else {
            return;
        };
        if key_state != WEnum::Value(wl_keyboard::KeyState::Pressed) {
            return;
        }
        if !state.locked {
            return;
        }
        let Some(started_at) = state.started_at else {
            return;
        };
        if started_at.elapsed() < Duration::from_millis(1500) {
            return;
        }
        if !state.activity_sent {
            state.activity_sent = true;
            let _ = state.sender.send(AppMessage::StopScreensaverUserActivity);
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if !state.locked {
            return;
        }
        let Some(started_at) = state.started_at else {
            return;
        };
        if started_at.elapsed() < Duration::from_millis(state.mouse_wake_delay_ms) {
            return;
        }
        match event {
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.last_pointer_pos = Some((surface_x, surface_y));
                if !state.activity_sent {
                    state.activity_sent = true;
                    let _ = state.sender.send(AppMessage::StopScreensaverUserActivity);
                }
            }
            wl_pointer::Event::Button {
                state: btn_state, ..
            } if btn_state == WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                if !state.activity_sent {
                    state.activity_sent = true;
                    let _ = state.sender.send(AppMessage::StopScreensaverUserActivity);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_output::WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_session_lock_manager_v1::ExtSessionLockManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ext_session_lock_manager_v1::ExtSessionLockManagerV1,
        _: ext_session_lock_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => {
                state.locked = true;
                state.started_at = Some(Instant::now());
                if let Some(tx) = state.ready_tx.take() {
                    let _ = tx.send(true);
                }
            }
            ext_session_lock_v1::Event::Finished => {
                if !state.locked {
                    if let Some(tx) = state.ready_tx.take() {
                        let _ = tx.send(false);
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
struct LockSurfaceKey {
    idx: usize,
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, LockSurfaceKey> for State {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        data: &LockSurfaceKey,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        else {
            return;
        };

        let width = width.max(1);
        let height = height.max(1);

        let idx = data.idx;
        let Some(lock_surface_state) = state.lock_surfaces.get_mut(idx) else {
            return;
        };

        lock_surface_state.last_serial = Some(serial);
        lock_surface_state.lock_surface.ack_configure(serial);

        let Some(shm) = state.shm.as_ref() else {
            return;
        };

        let needs_new = lock_surface_state
            .shm_buffer
            .as_ref()
            .map(|b| b.width != width || b.height != height)
            .unwrap_or(true);

        if needs_new {
            let Ok(mut buf) = ShmBuffer::new(shm, width, height, qh) else {
                return;
            };
            state
                .background
                .fill(buf.pixels_mut(), width as u32, height as u32);
            lock_surface_state.shm_buffer = Some(buf);
        } else if let Some(buf) = lock_surface_state.shm_buffer.as_mut() {
            state
                .background
                .fill(buf.pixels_mut(), width as u32, height as u32);
        }

        let Some(buf) = lock_surface_state.shm_buffer.as_ref() else {
            return;
        };

        lock_surface_state.surface.attach(Some(&buf.buffer), 0, 0);
        lock_surface_state
            .surface
            .damage_buffer(0, 0, width as i32, height as i32);
        lock_surface_state.surface.commit();
    }
}

pub fn start_wayland_session_lock_screensaver(
    profile: &SettingsProfile,
    sender: mpsc::Sender<AppMessage>,
) -> Option<SessionLockController> {
    // Quick probe on the calling thread so we can fall back to GTK immediately.
    {
        struct Probe {
            lock_manager_found: bool,
        }
        impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
            fn event(
                state: &mut Self,
                _: &wl_registry::WlRegistry,
                event: wl_registry::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                if let wl_registry::Event::Global { interface, .. } = event {
                    if interface == "ext_session_lock_manager_v1" {
                        state.lock_manager_found = true;
                    }
                }
            }
        }
        let conn = Connection::connect_to_env().ok()?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let mut probe = Probe {
            lock_manager_found: false,
        };
        conn.display().get_registry(&qh, ());
        let _ = queue.roundtrip(&mut probe).ok()?;
        if !probe.lock_manager_found {
            return None;
        }
    }

    let background = Background::from_mode(&profile.mode);
    let mouse_wake_delay_ms = profile.mouse_wake_delay_ms;

    let (stop_read_fd, stop_write_fd) = create_pipe().ok()?;
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();

    thread::spawn(move || {
        let Ok(conn) = Connection::connect_to_env() else {
            unsafe {
                let _ = libc::close(stop_read_fd);
            }
            return;
        };
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();

        let mut state = State {
            compositor: None,
            shm: None,
            seat: None,
            outputs: Vec::new(),
            lock_manager: None,
            lock: None,
            lock_surfaces: Vec::new(),
            default_background: background.clone(),
            background,
            locked: false,
            ready_tx: Some(ready_tx),
            activity_sent: false,
            sender,
            started_at: None,
            mouse_wake_delay_ms,
            last_pointer_pos: None,
            stop_read_fd,
            keyboard: None,
            pointer: None,
        };

        conn.display().get_registry(&qh, ());
        if queue.roundtrip(&mut state).is_err() {
            if let Some(tx) = state.ready_tx.take() {
                let _ = tx.send(false);
            }
            unsafe {
                let _ = libc::close(state.stop_read_fd);
            }
            return;
        }

        let Some(compositor) = state.compositor.clone() else {
            if let Some(tx) = state.ready_tx.take() {
                let _ = tx.send(false);
            }
            unsafe {
                let _ = libc::close(state.stop_read_fd);
            }
            return;
        };
        let Some(lock_manager) = state.lock_manager.clone() else {
            if let Some(tx) = state.ready_tx.take() {
                let _ = tx.send(false);
            }
            unsafe {
                let _ = libc::close(state.stop_read_fd);
            }
            return;
        };

        let lock = lock_manager.lock(&qh, ());
        state.lock = Some(lock);

        // Create lock surfaces for all known outputs.
        for (idx, output) in state.outputs.iter().enumerate() {
            let surface = compositor.create_surface(&qh, ());
            let lock_surface = state.lock.as_ref().unwrap().get_lock_surface(
                &surface,
                output,
                &qh,
                LockSurfaceKey { idx },
            );
            state.lock_surfaces.push(LockSurfaceState {
                surface,
                lock_surface,
                shm_buffer: None,
                last_serial: None,
            });
        }

        let mut stop_buf = [0u8; 64];
        let ready_deadline = Instant::now() + Duration::from_millis(2000);
        loop {
            // Flush outgoing requests and process already queued events.
            let _ = queue.flush();
            let _ = queue.dispatch_pending(&mut state);

            // If the compositor denied the lock request.
            if state.ready_tx.is_none() && !state.locked {
                break;
            }
            // If the lock doesn't arrive soon, fall back.
            if !state.locked && Instant::now() >= ready_deadline {
                if let Some(tx) = state.ready_tx.take() {
                    let _ = tx.send(false);
                }
                break;
            }
            // If activity was observed, unlock and exit.
            if state.activity_sent {
                break;
            }

            let Some(read_guard) = queue.prepare_read() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };

            let mut fds = [
                libc::pollfd {
                    fd: conn.as_fd().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: state.stop_read_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];

            let poll_res = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 250) };
            if poll_res < 0 {
                std::mem::drop(read_guard);
                break;
            }

            let stop_ready = (fds[1].revents & libc::POLLIN) != 0;
            if stop_ready {
                let n = unsafe {
                    libc::read(
                        state.stop_read_fd,
                        stop_buf.as_mut_ptr() as *mut _,
                        stop_buf.len(),
                    )
                };
                std::mem::drop(read_guard);
                if n > 0 {
                    let mut should_stop = false;
                    for &b in stop_buf.iter().take(n as usize) {
                        match b {
                            b'S' => should_stop = true,
                            b'B' => {
                                state.background = Background::Solid(Color::BLACK);
                                repaint_all_lock_surfaces(&mut state);
                            }
                            b'D' => {
                                state.background = state.default_background.clone();
                                repaint_all_lock_surfaces(&mut state);
                            }
                            _ => {}
                        }
                    }
                    if should_stop {
                        break;
                    }
                    continue;
                } else {
                    break;
                }
            }

            let wayland_ready = (fds[0].revents & libc::POLLIN) != 0;
            if wayland_ready {
                let _ = read_guard.read();
                let _ = queue.dispatch_pending(&mut state);
            } else {
                std::mem::drop(read_guard);
            }
        }

        // Unlock/cleanup.
        if let Some(lock) = state.lock.take() {
            if state.locked {
                lock.unlock_and_destroy();
            } else {
                lock.destroy();
            }
            // Ensure the compositor processes unlock.
            let _ = queue.roundtrip(&mut state);
        }

        unsafe {
            let _ = libc::close(state.stop_read_fd);
        }
    });

    match ready_rx.recv_timeout(Duration::from_millis(2300)) {
        Ok(true) => Some(SessionLockController {
            stop_fd: stop_write_fd,
        }),
        _ => {
            unsafe {
                let _ = libc::close(stop_write_fd);
            }
            None
        }
    }
}

fn repaint_all_lock_surfaces(state: &mut State) {
    for s in state.lock_surfaces.iter_mut() {
        let Some(buf) = s.shm_buffer.as_mut() else {
            continue;
        };
        let (w, h) = (buf.width, buf.height);
        state.background.fill(buf.pixels_mut(), w, h);
        s.surface.attach(Some(&buf.buffer), 0, 0);
        s.surface.damage_buffer(0, 0, w as i32, h as i32);
        s.surface.commit();
    }
}
