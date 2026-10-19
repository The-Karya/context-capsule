// context-capsule — linux: the PipeWire stream of the picked content. Keeps only the latest frame,
// BGRA in memory, at most about one per second, stamped with when it arrived. Nothing touches disk.

use context_capsule_core::Frame;
use pipewire as pw;
use pw::spa;
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::pod::Pod;
use std::cell::RefCell;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// At most one frame copy per this long. Newer frames wait (held, not copied) until it has passed.
pub const MIN_INTERVAL: Duration = Duration::from_millis(1000);
/// How often a held frame is checked.
const TICK: Duration = Duration::from_millis(250);

/// The latest frame, shared with the engine's thread.
#[derive(Default)]
pub struct FrameSlot {
    latest: Mutex<Option<Frame>>,
}

impl FrameSlot {
    pub fn latest(&self) -> Option<Frame> {
        self.latest.lock().unwrap().clone()
    }
    pub fn clear(&self) {
        *self.latest.lock().unwrap() = None;
    }
    fn put(&self, frame: Frame) {
        *self.latest.lock().unwrap() = Some(frame);
    }
}

/// A running capture. Dropping it stops the stream and its thread.
pub struct Stream {
    stop: pw::channel::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Is it time for another copy?
pub fn due(last_copy: Option<Instant>, now: Instant) -> bool {
    last_copy.is_none_or(|t| now.duration_since(t) >= MIN_INTERVAL)
}

/// Copy `height` rows of a frame into tightly packed BGRA. RGB-ordered formats are swapped.
pub fn to_bgra(src: &[u8], width: usize, height: usize, stride: usize, rgb_order: bool) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || stride < width * 4 || src.len() < stride * (height - 1) + width * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &src[y * stride..y * stride + width * 4];
        if rgb_order {
            for px in row.as_chunks::<4>().0 {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        } else {
            out.extend_from_slice(row);
        }
    }
    Some(out)
}

/// Start the stream on its own thread. `on_end` runs once if the stream fails or ends by itself.
pub fn start(
    fd: OwnedFd,
    node: u32,
    slot: Arc<FrameSlot>,
    on_end: Box<dyn FnOnce(Option<String>) + Send>,
) -> Result<Stream, String> {
    let (stop, stop_rx) = pw::channel::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let thread = std::thread::Builder::new()
        .name("pipewire".into())
        .spawn(move || {
            if let Err(e) = run(fd, node, slot, on_end, stop_rx, &ready_tx) {
                let _ = ready_tx.send(Err(e));
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Stream { stop, thread: Some(thread) }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err("the capture thread ended".into()),
    }
}

struct State {
    format: Option<(VideoFormat, usize, usize)>,
    /// The newest buffer, dequeued but not yet copied, and when it arrived.
    held: Option<(*mut pw::sys::pw_buffer, SystemTime)>,
    last_copy: Option<Instant>,
    streamed: bool,
    on_end: Option<Box<dyn FnOnce(Option<String>) + Send>>,
}

fn run(
    fd: OwnedFd,
    node: u32,
    slot: Arc<FrameSlot>,
    on_end: Box<dyn FnOnce(Option<String>) + Send>,
    stop: pw::channel::Receiver<()>,
    ready: &std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context.connect_fd_rc(fd, None).map_err(|e| e.to_string())?;
    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
    };
    let stream = pw::stream::StreamRc::new(core, "context-capsule", props).map_err(|e| e.to_string())?;
    let state = Rc::new(RefCell::new(State {
        format: None,
        held: None,
        last_copy: None,
        streamed: false,
        on_end: Some(on_end),
    }));

    let end = {
        let (state, mainloop) = (state.clone(), mainloop.clone());
        move |why: Option<String>| {
            if let Some(f) = state.borrow_mut().on_end.take() {
                f(why);
            }
            mainloop.quit();
        }
    };
    let _listener = stream
        .add_local_listener::<()>()
        .state_changed({
            let (state, end) = (state.clone(), end.clone());
            move |_, _, _, new| match new {
                pw::stream::StreamState::Streaming => state.borrow_mut().streamed = true,
                pw::stream::StreamState::Error(e) => end(Some(e)),
                pw::stream::StreamState::Unconnected if state.borrow().streamed => end(None),
                _ => {}
            }
        })
        .param_changed({
            let state = state.clone();
            move |_, _, id, param| {
                let Some(param) = param else { return };
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                let mut info = VideoInfoRaw::default();
                if info.parse(param).is_ok() {
                    let size = info.size();
                    state.borrow_mut().format = Some((info.format(), size.width as usize, size.height as usize));
                }
            }
        })
        .process({
            let (state, slot) = (state.clone(), slot.clone());
            move |stream, _| {
                // Keep only the newest buffer; give older ones straight back.
                loop {
                    let raw = unsafe { stream.dequeue_raw_buffer() };
                    if raw.is_null() {
                        break;
                    }
                    let old = state.borrow_mut().held.replace((raw, SystemTime::now()));
                    if let Some((old, _)) = old {
                        unsafe { stream.queue_raw_buffer(old) };
                    }
                }
                copy_if_due(stream, &state, &slot);
            }
        })
        .register()
        .map_err(|e| e.to_string())?;

    let params = format_params();
    let mut pods: Vec<&Pod> = params.iter().filter_map(|b| Pod::from_bytes(b)).collect();
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut pods,
        )
        .map_err(|e| e.to_string())?;

    // A held frame is copied once a second has passed, even if nothing new arrives.
    let timer = mainloop.loop_().add_timer({
        let (state, slot, stream) = (state.clone(), slot.clone(), stream.clone());
        move |_| copy_if_due(&stream, &state, &slot)
    });
    let _ = timer.update_timer(Some(TICK), Some(TICK));
    let _stop = stop.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let _ = ready.send(Ok(()));
    mainloop.run();

    if let Some((raw, _)) = state.borrow_mut().held.take() {
        unsafe { stream.queue_raw_buffer(raw) };
    }
    let _ = stream.disconnect();
    Ok(())
}

fn copy_if_due(stream: &pw::stream::Stream, state: &Rc<RefCell<State>>, slot: &FrameSlot) {
    let mut s = state.borrow_mut();
    if s.held.is_none() || !due(s.last_copy, Instant::now()) {
        return;
    }
    let Some((raw, arrived)) = s.held.take() else { return };
    if let Some(frame) = s.format.and_then(|f| unsafe { frame_of(raw, f, arrived) }) {
        slot.put(frame);
        s.last_copy = Some(Instant::now());
    }
    unsafe { stream.queue_raw_buffer(raw) };
}

/// The frame in a mapped buffer, copied out. None for empty, corrupted or unmapped (DMA-BUF) buffers.
///
/// # Safety
/// `raw` must be a buffer dequeued from a stream connected with `MAP_BUFFERS`, not yet queued back.
unsafe fn frame_of(
    raw: *mut pw::sys::pw_buffer,
    (format, width, height): (VideoFormat, usize, usize),
    arrived: SystemTime,
) -> Option<Frame> {
    let buffer = (*raw).buffer;
    if buffer.is_null() || (*buffer).n_datas == 0 || (*buffer).datas.is_null() {
        return None;
    }
    let data = &*(*buffer).datas;
    if data.data.is_null() || data.chunk.is_null() {
        return None;
    }
    let chunk = &*data.chunk;
    if chunk.size == 0 || chunk.flags & spa::sys::SPA_CHUNK_FLAG_CORRUPTED as i32 != 0 {
        return None;
    }
    let offset = (chunk.offset % data.maxsize.max(1)) as usize;
    let len = (chunk.size as usize).min(data.maxsize as usize - offset);
    let bytes = std::slice::from_raw_parts((data.data as *const u8).add(offset), len);
    let stride = if chunk.stride > 0 { chunk.stride as usize } else { width * 4 };
    let rgb_order = match format {
        VideoFormat::BGRx | VideoFormat::BGRA => false,
        VideoFormat::RGBx | VideoFormat::RGBA => true,
        _ => return None,
    };
    let bgra = to_bgra(bytes, width, height, stride, rgb_order)?;
    Some(Frame { bgra: bgra.into(), width, height, stride: width * 4, arrived })
}

/// What we accept: raw 32-bit RGB in memory (no DMA-BUF modifiers, so the compositor sends mapped
/// memory). First choice asks for at most 1 frame/s; the second takes any rate (we throttle).
fn format_params() -> Vec<Vec<u8>> {
    use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
    use spa::pod::{property, Value};
    use spa::utils::{Fraction, Rectangle, SpaTypes};
    let build = |limit_rate: bool| {
        let mut obj = spa::pod::object!(
            SpaTypes::ObjectParamFormat,
            spa::param::ParamType::EnumFormat,
            property!(FormatProperties::MediaType, Id, MediaType::Video),
            property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            property!(
                FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                VideoFormat::BGRx,
                VideoFormat::BGRx,
                VideoFormat::BGRA,
                VideoFormat::RGBx,
                VideoFormat::RGBA
            ),
            property!(
                FormatProperties::VideoSize,
                Choice,
                Range,
                Rectangle,
                Rectangle { width: 1920, height: 1080 },
                Rectangle { width: 1, height: 1 },
                Rectangle { width: 16384, height: 16384 }
            ),
            property!(
                FormatProperties::VideoFramerate,
                Choice,
                Range,
                Fraction,
                Fraction { num: 0, denom: 1 },
                Fraction { num: 0, denom: 1 },
                Fraction { num: 1000, denom: 1 }
            ),
        );
        if limit_rate {
            obj.properties.push(property!(
                FormatProperties::VideoMaxFramerate,
                Choice,
                Range,
                Fraction,
                Fraction { num: 1, denom: 1 },
                Fraction { num: 0, denom: 1 },
                Fraction { num: 1, denom: 1 }
            ));
        }
        spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj))
            .map(|(c, _)| c.into_inner())
            .unwrap_or_default()
    };
    vec![build(true), build(false)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_one_copy_a_second() {
        let t = Instant::now();
        assert!(due(None, t));
        assert!(!due(Some(t), t + Duration::from_millis(400)));
        assert!(due(Some(t), t + MIN_INTERVAL));
    }

    #[test]
    fn pixels_to_bgra() {
        // 2×2, stride 12 (4 bytes padding per row), RGBx order.
        let src = [1, 2, 3, 0, 4, 5, 6, 0, 9, 9, 9, 9, 7, 8, 9, 0, 10, 11, 12, 0, 9, 9, 9, 9];
        let out = to_bgra(&src, 2, 2, 12, true).unwrap();
        assert_eq!(out, vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]);
        let same = to_bgra(&src, 2, 2, 12, false).unwrap();
        assert_eq!(&same[..8], &src[..8], "BGRx copied as is, padding dropped");
        assert!(to_bgra(&src[..10], 2, 2, 12, false).is_none(), "short buffer");
        assert!(to_bgra(&src, 4, 2, 12, false).is_none(), "stride too small");
    }

    #[test]
    fn format_pods_parse() {
        let params = format_params();
        assert_eq!(params.len(), 2);
        for p in &params {
            assert!(Pod::from_bytes(p).is_some());
        }
        assert!(params[0].len() > params[1].len(), "the first also limits the frame rate");
    }
}
