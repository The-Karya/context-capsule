// context-capsule — windows: the core's `Platform` on Windows. Runs on the engine thread; anything that
// must happen on the UI thread (picker, tray, toasts, alerts) is posted there.

use crate::app::{Ui, UiCmd};
use crate::ocr::Ocr;
use crate::picker::Capture;
use crate::uia::Uia;
use crate::{changes, windows_list};
use context_capsule_core::{Frame, Front, Image, Indicator, PickedContent, Platform, Status};
use std::sync::Arc;
use std::time::SystemTime;

pub struct WinPlatform {
    ui: Ui,
    capture: Arc<Capture>,
    apps: windows_list::Apps,
    uia: Option<Uia>,
    uia_tried: bool,
    ocr: Ocr,
}

impl WinPlatform {
    pub fn new(ui: Ui, capture: Arc<Capture>) -> WinPlatform {
        WinPlatform { ui, capture, apps: Default::default(), uia: None, uia_tried: false, ocr: Ocr::default() }
    }

    fn uia(&mut self) -> Option<&Uia> {
        if !self.uia_tried {
            self.uia_tried = true;
            self.uia = Uia::new();
            if self.uia.is_none() {
                eprintln!("Context Capsule: UI Automation is not available; text comes from OCR");
            }
        }
        self.uia.as_ref()
    }
}

impl Platform for WinPlatform {
    fn present_picker(&mut self) {
        self.ui.post(UiCmd::PresentPicker);
    }

    fn stop_capture(&mut self) {
        self.capture.stop();
    }

    fn picked(&self) -> Option<PickedContent> {
        self.capture.picked()
    }

    fn latest_frame(&self) -> Option<Frame> {
        self.capture.latest_frame()
    }

    fn front(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        windows_list::front(&mut self.apps, is_excluded)
    }

    fn ui_text(&mut self, front: &Front) -> Option<String> {
        let id = front.window.as_ref()?.id;
        let hwnd = windows::Win32::Foundation::HWND(id as usize as *mut core::ffi::c_void);
        self.uia()?.text(hwnd)
    }

    fn ui_title(&mut self, front: &Front) -> Option<String> {
        front.window.as_ref().map(|w| windows_list::title(w.id))
    }

    fn ocr(&mut self, image: &Image) -> String {
        self.ocr.read(image)
    }

    fn changed_at(&self) -> Option<SystemTime> {
        changes::last()
    }

    fn set_indicator(&mut self, indicator: Indicator) {
        self.ui.post(UiCmd::Indicator(indicator));
    }

    fn toast(&mut self, text: &str) {
        self.ui.post(UiCmd::Toast(text.to_string()));
    }

    fn alert(&mut self, title: &str, body: &str) {
        self.ui.post(UiCmd::Alert(title.to_string(), body.to_string()));
    }

    fn host_name(&self) -> String {
        std::env::var("COMPUTERNAME").ok().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| "Windows PC".into())
    }

    fn status_changed(&mut self, status: &Status) {
        self.ui.post(UiCmd::Status(Box::new(status.clone())));
    }

    /// Browsers only: the address bar through UI Automation. Other apps: None.
    fn location(&mut self, front: &Front) -> Option<String> {
        let browser = crate::uia::browser_for(front.app_id.as_deref())?;
        let id = front.window.as_ref()?.id;
        let hwnd = windows::Win32::Foundation::HWND(id as usize as *mut core::ffi::c_void);
        self.uia()?.address(hwnd, browser)
    }
}
