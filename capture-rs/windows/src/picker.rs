// context-capsule — windows: the system capture picker (GraphicsCapturePicker) and the captures of what the
// user picked: one at the start, and up to 4 in all through "Add another screen…" (the picker once per added
// screen or window). The user's choice is the consent; we capture only that, keep only the latest frame of
// each, in memory, and never write it to disk.
//! Windows draws its yellow border around what is shared; we keep it (it tells the user what is read).

use context_capsule_core::{Membership, PickedContent, Rect};

/// An on-screen window the picked item might be.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowCandidate {
    pub id: u64,
    pub title: String,
    pub frame: Rect,
}

/// A monitor the picked item might be. `device` is its name ("\\.\DISPLAY1").
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorCandidate {
    pub device: String,
    pub rect: Rect,
}

fn same_size(r: &Rect, width: f64, height: f64) -> bool {
    let t = Membership::TOLERANCE;
    (r.width - width).abs() <= t && (r.height - height).abs() <= t
}

/// What the picker's item is: a window (title and size match exactly one window, and no monitor by
/// name) or a display (named after one monitor, or the only monitor of that size when no window
/// matches). Anything else fails closed: a display with no known place, so nothing is read
/// (the engine skips with `unknown_display`).
pub fn resolve_pick(name: &str, width: f64, height: f64, windows: &[WindowCandidate], monitors: &[MonitorCandidate]) -> PickedContent {
    let unknown = PickedContent::display(Rect::ZERO);
    if width <= 0.0 || height <= 0.0 {
        return unknown;
    }
    let wins: Vec<&WindowCandidate> =
        windows.iter().filter(|w| !name.is_empty() && w.title == name && same_size(&w.frame, width, height)).collect();
    let named: Vec<&MonitorCandidate> =
        monitors.iter().filter(|m| !name.is_empty() && m.device.eq_ignore_ascii_case(name)).collect();
    let sized: Vec<&MonitorCandidate> = monitors.iter().filter(|m| same_size(&m.rect, width, height)).collect();

    match (wins.as_slice(), named.as_slice()) {
        ([w], []) => PickedContent::window(w.frame, Some(vec![w.id])),
        ([], [m]) => PickedContent::display(m.rect),
        ([], []) if sized.len() == 1 && !windows.iter().any(|w| w.title == name && !name.is_empty()) => {
            PickedContent::display(sized[0].rect)
        }
        _ => unknown,
    }
}

#[cfg(windows)]
pub use win::Capture;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::windows_list;
    use context_capsule_core::{Frame, Msg, PickOutcome, MAX_SOURCES};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex, MutexGuard, Weak};
    use std::time::SystemTime;
    use windows::core::{AgileReference, IInspectable, Interface, Ref};
    use windows::Foundation::{TimeSpan, TypedEventHandler};
    use windows::Graphics::Capture::{
        Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCapturePicker, GraphicsCaptureSession,
    };
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Foundation::{HMODULE, HWND};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Multithread, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
        D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
    use windows::Win32::UI::Shell::IInitializeWithWindow;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    use windows_future::IAsyncOperation;

    const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
    /// Frames come only when the content changes; ask for at most one a second where Windows allows it.
    const MIN_INTERVAL: TimeSpan = TimeSpan { Duration: 10_000_000 };

    struct Live {
        item: GraphicsCaptureItem,
        pool: Direct3D11CaptureFramePool,
        session: GraphicsCaptureSession,
        /// For new buffers when the content is resized (callbacks come on other threads).
        device: AgileReference<IDirect3DDevice>,
        size: SizeInt32,
        closed_token: i64,
    }

    impl Live {
        fn close(self) {
            let _ = self.item.RemoveClosed(self.closed_token);
            let _ = self.session.Close();
            let _ = self.pool.Close();
        }
    }

    /// One shared screen or window: its capture, what it is, and its latest frame.
    struct Source {
        id: u32,
        live: Live,
        picked: PickedContent,
        latest: Option<(Direct3D11CaptureFrame, SystemTime)>,
    }

    #[derive(Default)]
    struct Inner {
        /// Bumped by every stop: callbacks from an older picker or capture are ignored.
        generation: u64,
        picking: bool,
        /// The open picker adds a screen to a running recording.
        adding: bool,
        op: Option<IAsyncOperation<GraphicsCaptureItem>>,
        sources: Vec<Source>,
        next_id: u32,
    }

    /// Shared by the UI thread (opens the picker), WinRT callbacks (frames, the item closing) and the
    /// engine thread (picked, latest frame, stop).
    pub struct Capture {
        inner: Mutex<Inner>,
        tx: Sender<Msg>,
    }

    impl Capture {
        pub fn new(tx: Sender<Msg>) -> Arc<Capture> {
            Arc::new(Capture { inner: Mutex::new(Inner::default()), tx })
        }

        fn lock(&self) -> MutexGuard<'_, Inner> {
            self.inner.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn report(&self, outcome: PickOutcome) {
            let _ = self.tx.send(Msg::Pick(outcome));
        }

        /// Show the picker (UI thread). If it is already open or something is being captured, do nothing.
        pub fn present(self: &Arc<Self>, owner: HWND) {
            self.open(owner, false);
        }

        /// While capturing: the picker again, for one more screen or window (up to `MAX_SOURCES`).
        pub fn present_add(self: &Arc<Self>, owner: HWND) {
            self.open(owner, true);
        }

        fn open(self: &Arc<Self>, owner: HWND, adding: bool) {
            let generation = {
                let mut inner = self.lock();
                let ready = if adding {
                    !inner.sources.is_empty() && inner.sources.len() < MAX_SOURCES
                } else {
                    inner.sources.is_empty()
                };
                if inner.picking || !ready {
                    return;
                }
                inner.picking = true;
                inner.adding = adding;
                inner.generation
            };
            match self.open_picker(owner, generation) {
                Ok(op) => {
                    let mut inner = self.lock();
                    if inner.picking && inner.generation == generation {
                        inner.op = Some(op);
                    }
                }
                Err(e) => {
                    self.lock().picking = false;
                    self.report(if adding {
                        PickOutcome::AddCancelled
                    } else {
                        PickOutcome::Failed(format!("picker: {}", e.message()))
                    });
                }
            }
        }

        fn open_picker(self: &Arc<Self>, owner: HWND, generation: u64) -> windows::core::Result<IAsyncOperation<GraphicsCaptureItem>> {
            if !GraphicsCaptureSession::IsSupported()? {
                return Err(windows::core::Error::new(
                    windows::Win32::Foundation::E_NOTIMPL,
                    "screen capture is not supported on this PC",
                ));
            }
            let picker = GraphicsCapturePicker::new()?;
            // SAFETY: ties the picker to our (hidden) window, as desktop apps must.
            unsafe {
                let _ = SetForegroundWindow(owner);
                picker.cast::<IInitializeWithWindow>()?.Initialize(owner)?;
            }
            let op = picker.PickSingleItemAsync()?;
            let me = Arc::downgrade(self);
            op.when(move |result| {
                if let Some(me) = me.upgrade() {
                    me.picked_item(generation, result);
                }
            })?;
            Ok(op)
        }

        /// The picker closed: an item (start capturing it), nothing (cancelled) or an error.
        fn picked_item(self: &Arc<Self>, generation: u64, result: windows::core::Result<GraphicsCaptureItem>) {
            let mut inner = self.lock();
            if inner.generation != generation || !inner.picking {
                return; // cancelled from our side meanwhile
            }
            inner.picking = false;
            inner.op = None;
            let adding = std::mem::take(&mut inner.adding);
            let outcome = match result {
                // A closed picker gives no item (a null result).
                Err(e) if e.code().is_ok() => PickOutcome::Cancelled,
                Err(e) => PickOutcome::Failed(format!("picker: {}", e.message())),
                Ok(item) => match self.start(&mut inner, generation, item) {
                    Ok(()) => PickOutcome::Picked,
                    Err(e) => PickOutcome::Failed(format!("capture: {}", e.message())),
                },
            };
            let outcome = match outcome {
                PickOutcome::Picked if adding => PickOutcome::Added(inner.sources.len()),
                PickOutcome::Cancelled | PickOutcome::Failed(_) if adding => {
                    if let PickOutcome::Failed(why) = &outcome {
                        eprintln!("Context Capsule: add a screen failed: {why}");
                    }
                    PickOutcome::AddCancelled
                }
                other => other,
            };
            drop(inner);
            self.report(outcome);
        }

        fn start(self: &Arc<Self>, inner: &mut Inner, generation: u64, item: GraphicsCaptureItem) -> windows::core::Result<()> {
            let size = item.Size()?;
            let id = inner.next_id.max(1);
            inner.next_id = id + 1;
            let picked = describe(&item.DisplayName()?.to_string_lossy(), size).with_source(id);
            let device = d3d_device()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device, FORMAT, 2, size)?;
            let me: Weak<Capture> = Arc::downgrade(self);
            pool.FrameArrived(&TypedEventHandler::new(move |pool: Ref<Direct3D11CaptureFramePool>, _: Ref<IInspectable>| {
                if let (Some(me), Some(pool)) = (me.upgrade(), pool.as_ref()) {
                    me.frame_arrived(generation, id, pool);
                }
                Ok(())
            }))?;
            let session = pool.CreateCaptureSession(&item)?;
            let _ = session.SetIsCursorCaptureEnabled(false); // the pointer only gets in the way of OCR
            let _ = session.SetMinUpdateInterval(MIN_INTERVAL); // Windows 11 24H2+; older: frames on change
            let me: Weak<Capture> = Arc::downgrade(self);
            let closed_token = item.Closed(&TypedEventHandler::new(move |_: Ref<GraphicsCaptureItem>, _: Ref<IInspectable>| {
                if let Some(me) = me.upgrade() {
                    me.ended(generation, id, None); // the window closed, or the user stopped sharing
                }
                Ok(())
            }))?;
            session.StartCapture()?;
            let device = AgileReference::new(&device)?;
            let live = Live { item, pool, session, device, size, closed_token };
            inner.sources.push(Source { id, live, picked, latest: None });
            Ok(())
        }

        /// Keep only the newest frame (not copied yet: `latest_frame` copies on demand).
        fn frame_arrived(&self, generation: u64, id: u32, pool: &Direct3D11CaptureFramePool) {
            let frame = match pool.TryGetNextFrame() {
                Ok(f) => f,
                Err(_) => return,
            };
            let content = frame.ContentSize().unwrap_or_default();
            let mut inner = self.lock();
            if inner.generation != generation {
                return;
            }
            let Some(source) = inner.sources.iter_mut().find(|s| s.id == id) else { return };
            let live = &mut source.live;
            if content.Width > 0 && content.Height > 0 && content != live.size {
                // The window was resized: new buffers for the new size.
                if let Err(e) = live.device.resolve().and_then(|d| live.pool.Recreate(&d, FORMAT, 2, content)) {
                    let why = format!("capture: {}", e.message());
                    drop(inner);
                    return self.ended(generation, id, Some(why));
                }
                live.size = content;
            }
            source.latest = Some((frame, SystemTime::now()));
        }

        /// One capture ended on its own (None: the user or Windows stopped it). Others go on; when it
        /// was the last one, the recording stops.
        fn ended(&self, generation: u64, id: u32, why: Option<String>) {
            let (source, left) = {
                let mut inner = self.lock();
                if inner.generation != generation {
                    return;
                }
                let Some(at) = inner.sources.iter().position(|s| s.id == id) else { return };
                let source = inner.sources.remove(at);
                if inner.sources.is_empty() {
                    inner.generation += 1;
                }
                (source, inner.sources.len())
            };
            source.live.close();
            self.report(if left > 0 { PickOutcome::Removed(left) } else { PickOutcome::Stopped(why) });
        }

        /// Stop capturing and cancel an open picker. Safe in any state; reports nothing.
        pub fn stop(&self) {
            let (op, sources) = {
                let mut inner = self.lock();
                inner.generation += 1;
                inner.picking = false;
                inner.adding = false;
                (inner.op.take(), std::mem::take(&mut inner.sources))
            };
            // Outside the lock: these may run our callbacks.
            if let Some(op) = op {
                let _ = op.Cancel();
            }
            for s in sources {
                s.live.close();
            }
        }

        /// Everything shared, each with its source id.
        pub fn picked(&self) -> Vec<PickedContent> {
            self.lock().sources.iter().map(|s| s.picked.clone()).collect()
        }

        /// The latest frame of one source as BGRA (copied through a staging texture, then dropped by the caller).
        pub fn latest_frame(&self, source: u32) -> Option<Frame> {
            let (frame, arrived) = self.lock().sources.iter().find(|s| s.id == source)?.latest.clone()?;
            match copy_frame(&frame) {
                Ok((bgra, width, height)) => Some(Frame { bgra: bgra.into(), width, height, stride: width * 4, arrived }),
                Err(e) => {
                    eprintln!("Context Capsule: cannot read the frame: {}", e.message());
                    None
                }
            }
        }
    }

    /// What was picked, placed on screen (physical pixels).
    fn describe(name: &str, size: SizeInt32) -> PickedContent {
        let windows: Vec<WindowCandidate> = windows_list::on_screen()
            .into_iter()
            .map(|w| WindowCandidate { id: w.id, title: w.title, frame: w.frame })
            .collect();
        let pick = resolve_pick(name, size.Width as f64, size.Height as f64, &windows, &windows_list::monitors());
        eprintln!(
            "Context Capsule: picked a {} ({}x{}){}",
            if pick.window_ids.is_some() { "window" } else { "display" },
            size.Width,
            size.Height,
            if pick.rect.is_empty() { ", but cannot tell which one: nothing will be read" } else { "" }
        );
        pick
    }

    fn d3d_device() -> windows::core::Result<IDirect3DDevice> {
        let device = create_d3d(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create_d3d(D3D_DRIVER_TYPE_WARP))?;
        // Frames arrive on a pool thread while the engine thread copies them.
        if let Ok(mt) = device.cast::<ID3D11Multithread>() {
            // SAFETY: a plain flag on our own device.
            unsafe {
                let _ = mt.SetMultithreadProtected(true);
            }
        }
        let dxgi: IDXGIDevice = device.cast()?;
        // SAFETY: wraps our DXGI device for WinRT.
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? };
        inspectable.cast()
    }

    fn create_d3d(driver: D3D_DRIVER_TYPE) -> windows::core::Result<ID3D11Device> {
        let mut device = None;
        // SAFETY: standard device creation; out-pointer is a local Option.
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
        }
        device.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
    }

    /// GPU frame → CPU BGRA, tightly packed, cropped to the content size.
    fn copy_frame(frame: &Direct3D11CaptureFrame) -> windows::core::Result<(Vec<u8>, usize, usize)> {
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: the surface is a D3D11 texture (we made the pool with a D3D11 device).
        let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: fills `desc`.
        unsafe { texture.GetDesc(&mut desc) };
        let content = frame.ContentSize()?;
        let width = (content.Width.max(0) as u32).min(desc.Width) as usize;
        let height = (content.Height.max(0) as u32).min(desc.Height) as usize;
        if width == 0 || height == 0 {
            return Err(windows::Win32::Foundation::E_FAIL.into());
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        // SAFETY: COM calls on our device; the staging texture lives until the end of this function.
        unsafe {
            let device = texture.GetDevice()?;
            let context = device.GetImmediateContext()?;
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
            context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let pitch = mapped.RowPitch as usize;
            let src = std::slice::from_raw_parts(mapped.pData as *const u8, pitch * (height - 1) + width * 4);
            let mut out = Vec::with_capacity(width * height * 4);
            for row in src.chunks(pitch).take(height) {
                out.extend_from_slice(&row[..width * 4]);
            }
            context.Unmap(&staging, 0);
            Ok((out, width, height))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::PickStyle;

    fn win(id: u64, title: &str, x: f64, y: f64, w: f64, h: f64) -> WindowCandidate {
        WindowCandidate { id, title: title.into(), frame: Rect::new(x, y, w, h) }
    }
    fn mon(device: &str, x: f64, w: f64, h: f64) -> MonitorCandidate {
        MonitorCandidate { device: device.into(), rect: Rect::new(x, 0.0, w, h) }
    }

    fn monitors() -> Vec<MonitorCandidate> {
        vec![mon(r"\\.\DISPLAY1", 0.0, 2560.0, 1440.0), mon(r"\\.\DISPLAY2", 2560.0, 1920.0, 1080.0)]
    }

    #[test]
    fn a_window_by_title_and_size() {
        let wins = [win(7, "Pricing — Linear", 100.0, 80.0, 1200.0, 800.0), win(9, "Inbox", 0.0, 0.0, 1200.0, 800.0)];
        let p = resolve_pick("Pricing — Linear", 1203.0, 798.0, &wins, &monitors());
        assert_eq!(p.style, PickStyle::Window);
        assert_eq!(p.window_ids, Some(vec![7]));
        assert_eq!(p.rect, Rect::new(100.0, 80.0, 1200.0, 800.0));
    }

    #[test]
    fn a_display_by_device_name_or_unique_size() {
        let wins = [win(7, "Inbox", 0.0, 0.0, 800.0, 600.0)];
        let p = resolve_pick(r"\\.\DISPLAY2", 1920.0, 1080.0, &wins, &monitors());
        assert_eq!(p, PickedContent::display(Rect::new(2560.0, 0.0, 1920.0, 1080.0)));
        let p = resolve_pick("DELL U2720Q", 2560.0, 1440.0, &wins, &monitors());
        assert_eq!(p, PickedContent::display(Rect::new(0.0, 0.0, 2560.0, 1440.0)), "unique size, no window by that name");
    }

    #[test]
    fn a_fullscreen_window_is_still_a_window() {
        let wins = [win(3, "Movie", 2560.0, 0.0, 1920.0, 1080.0)];
        let p = resolve_pick("Movie", 1920.0, 1080.0, &wins, &monitors());
        assert_eq!(p.window_ids, Some(vec![3]));
    }

    #[test]
    fn unsure_fails_closed() {
        let unknown = PickedContent::display(Rect::ZERO);
        // Two windows with the same title and size.
        let twins = [win(1, "New Tab", 0.0, 0.0, 1000.0, 700.0), win(2, "New Tab", 50.0, 50.0, 1000.0, 700.0)];
        assert_eq!(resolve_pick("New Tab", 1000.0, 700.0, &twins, &monitors()), unknown);
        // Two monitors of the same size and a name that is neither.
        let same = [mon("A", 0.0, 1920.0, 1080.0), mon("B", 1920.0, 1920.0, 1080.0)];
        assert_eq!(resolve_pick("Generic Monitor", 1920.0, 1080.0, &[], &same), unknown);
        // A title that matches a window but not its size, on a monitor-sized item.
        let moved = [win(1, "Notes", 0.0, 0.0, 800.0, 600.0)];
        assert_eq!(resolve_pick("Notes", 2560.0, 1440.0, &moved, &monitors()), unknown);
        // Nothing matches at all.
        assert_eq!(resolve_pick("Gone", 640.0, 480.0, &moved, &monitors()), unknown);
        // No size.
        assert_eq!(resolve_pick(r"\\.\DISPLAY1", 0.0, 0.0, &[], &monitors()), unknown);
        // A window and a monitor both claim the name.
        let clash = [win(5, r"\\.\DISPLAY1", 0.0, 0.0, 2560.0, 1440.0)];
        assert_eq!(resolve_pick(r"\\.\DISPLAY1", 2560.0, 1440.0, &clash, &monitors()), unknown);
    }
}
