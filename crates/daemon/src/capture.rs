//! Grabs a small region of an output with wlr-screencopy, on a connection of its own so the main loop is
//! never blocked. Used to see which parts of a bar are actually painted.

use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

pub struct Image {
    pub w: usize,
    pub h: usize,
    pub stride: usize,
    /// B G R A bytes.
    pub data: Vec<u8>,
}

#[derive(Default)]
struct State {
    names: Vec<(u32, Option<String>)>,
    buffer: Option<(wl_shm::Format, u32, u32, u32)>,
    buffer_done: bool,
    ready: bool,
    failed: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(st: &mut Self, _: &wl_output::WlOutput, ev: wl_output::Event, g: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Name { name } = ev {
            if let Some(e) = st.names.iter_mut().find(|(id, _)| id == g) {
                e.1 = Some(name);
            }
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for State {
    fn event(st: &mut Self, _: &ZwlrScreencopyFrameV1, ev: zwlr_screencopy_frame_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            zwlr_screencopy_frame_v1::Event::Buffer { format, width, height, stride } => {
                if let WEnum::Value(f) = format {
                    if st.buffer.is_none() {
                        st.buffer = Some((f, width, height, stride));
                    }
                }
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => st.buffer_done = true,
            zwlr_screencopy_frame_v1::Event::Ready { .. } => st.ready = true,
            zwlr_screencopy_frame_v1::Event::Failed => st.failed = true,
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore ZwlrScreencopyManagerV1);

fn memfd(size: usize) -> Option<OwnedFd> {
    unsafe {
        let fd = libc::memfd_create(c"snowhush-capture".as_ptr(), libc::MFD_CLOEXEC);
        if fd < 0 {
            return None;
        }
        let fd = OwnedFd::from_raw_fd(fd);
        if libc::ftruncate(std::os::fd::AsRawFd::as_raw_fd(&fd), size as libc::off_t) != 0 {
            return None;
        }
        Some(fd)
    }
}

/// `region` is x, y, w, h in the output's logical pixels.
pub fn grab(output_name: &str, region: (i32, i32, i32, i32)) -> Option<Image> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let mut st = State::default();
    let mgr = globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ()).ok()?;
    let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()).ok()?;
    let mut outputs = Vec::new();
    globals.contents().with_list(|list| {
        for g in list.iter().filter(|g| g.interface == "wl_output") {
            st.names.push((g.name, None));
            outputs.push(globals.registry().bind::<wl_output::WlOutput, _, _>(g.name, g.version.min(4), &qh, g.name));
        }
    });
    queue.roundtrip(&mut st).ok()?;
    let idx = st.names.iter().position(|(_, n)| n.as_deref() == Some(output_name))?;
    let frame = mgr.capture_output_region(0, &outputs[idx], region.0, region.1, region.2, region.3, &qh, ());
    while st.buffer.is_none() || (frame.version() >= 3 && !st.buffer_done) {
        queue.blocking_dispatch(&mut st).ok()?;
        if st.failed {
            return None;
        }
    }
    let (format, w, h, stride) = st.buffer?;
    if !matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888) {
        return None;
    }
    let size = (stride * h) as usize;
    let fd = memfd(size)?;
    let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
    let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, &qh, ());
    frame.copy(&buffer);
    while !st.ready && !st.failed {
        queue.blocking_dispatch(&mut st).ok()?;
    }
    let result = if st.ready {
        unsafe {
            let p = libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ, libc::MAP_SHARED, std::os::fd::AsRawFd::as_raw_fd(&fd), 0);
            if p == libc::MAP_FAILED {
                None
            } else {
                let data = std::slice::from_raw_parts(p as *const u8, size).to_vec();
                libc::munmap(p, size);
                Some(Image { w: w as usize, h: h as usize, stride: stride as usize, data })
            }
        }
    } else {
        None
    };
    buffer.destroy();
    pool.destroy();
    frame.destroy();
    result
}
