// context-capsule — linux: the little of WebKitGTK 6.0 the shared window uses — a web context with our `capsule:`
// scheme, the script and message handler, the view, running a script, the navigation and context-menu signals.
//! WebKitGTK is loaded when the window is asked for (`dlopen`), not linked: Context Capsule starts on computers
//! without it, and says what to install when the window is opened. (The webkit6 crate also needs GTK 4.10; the app
//! runs on GTK 4.8.)

use context_capsule_core::bridge;
use gtk::glib::translate::{from_glib, from_glib_full, ToGlibPtr};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::OnceLock;

pub const SCHEME: &str = "capsule";
pub const ORIGIN: &str = "capsule://app/";
/// The library, as the distributions name it (package: see linux/README.md).
pub const LIBRARY: &str = "libwebkitgtk-6.0.so.4";

type P = *mut c_void;
type Callback = Option<unsafe extern "C" fn(P, P)>;
type Ready = Option<unsafe extern "C" fn(P, P, P)>;
type Destroy = Option<unsafe extern "C" fn(P)>;

// WebKit's enum values (stable since WebKitGTK 2.0).
const INJECT_TOP_FRAME: c_int = 1;
const INJECT_AT_DOCUMENT_START: c_int = 0;
const STYLE_LEVEL_USER: c_int = 0;
const NEW_WINDOW_ACTION: c_int = 1;

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// WebKitGTK's functions, found in the library.
        #[allow(non_snake_case)]
        struct Api { $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)* }

        impl Api {
            /// # Safety
            /// `lib` is a handle from dlopen; each symbol has the C signature declared here.
            unsafe fn load(lib: P) -> Option<Api> {
                Some(Api { $($name: {
                    // SAFETY: a NUL-terminated name looked up in a live handle.
                    let p = unsafe { libc::dlsym(lib, concat!(stringify!($name), "\0").as_ptr() as *const c_char) };
                    if p.is_null() {
                        eprintln!("Context Capsule: WebKitGTK has no {}", stringify!($name));
                        return None;
                    }
                    // SAFETY: a non-null symbol with this signature (WebKitGTK 6.0's API).
                    unsafe { std::mem::transmute::<P, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p) }
                },)* })
            }
        }
    };
}

api! {
    webkit_uri_scheme_request_get_uri: fn(P) -> *const c_char;
    webkit_uri_scheme_response_new: fn(P, i64) -> P;
    webkit_uri_scheme_response_set_content_type: fn(P, *const c_char);
    webkit_uri_scheme_response_set_status: fn(P, u32, *const c_char);
    webkit_uri_scheme_request_finish_with_response: fn(P, P);
    webkit_uri_scheme_request_finish_error: fn(P, P);
    webkit_web_context_new: fn() -> P;
    webkit_web_context_get_security_manager: fn(P) -> P;
    webkit_security_manager_register_uri_scheme_as_secure: fn(P, *const c_char);
    webkit_security_manager_register_uri_scheme_as_cors_enabled: fn(P, *const c_char);
    webkit_web_context_register_uri_scheme: fn(P, *const c_char, Callback, P, Destroy);
    webkit_settings_new: fn() -> P;
    webkit_settings_set_enable_developer_extras: fn(P, c_int);
    webkit_settings_set_javascript_can_access_clipboard: fn(P, c_int);
    webkit_settings_set_enable_back_forward_navigation_gestures: fn(P, c_int);
    webkit_user_content_manager_new: fn() -> P;
    webkit_user_content_manager_add_script: fn(P, P);
    webkit_user_content_manager_add_style_sheet: fn(P, P);
    webkit_user_content_manager_remove_all_style_sheets: fn(P);
    webkit_user_content_manager_register_script_message_handler: fn(P, *const c_char, *const c_char) -> c_int;
    webkit_user_script_new: fn(*const c_char, c_int, c_int, *const *const c_char, *const *const c_char) -> P;
    webkit_user_script_unref: fn(P);
    webkit_user_style_sheet_new: fn(*const c_char, c_int, c_int, *const *const c_char, *const *const c_char) -> P;
    webkit_user_style_sheet_unref: fn(P);
    jsc_value_to_string: fn(P) -> *mut c_char;
    webkit_web_view_get_type: fn() -> glib::ffi::GType;
    webkit_web_view_load_uri: fn(P, *const c_char);
    webkit_web_view_evaluate_javascript: fn(P, *const c_char, isize, *const c_char, *const c_char, P, Ready, P);
    webkit_web_view_set_background_color: fn(P, *const gdk::ffi::GdkRGBA);
    webkit_navigation_policy_decision_get_type: fn() -> glib::ffi::GType;
    webkit_navigation_policy_decision_get_navigation_action: fn(P) -> P;
    webkit_navigation_action_get_request: fn(P) -> P;
    webkit_uri_request_get_uri: fn(P) -> *const c_char;
    webkit_policy_decision_ignore: fn(P);
    webkit_hit_test_result_context_is_editable: fn(P) -> c_int;
    webkit_hit_test_result_context_is_selection: fn(P) -> c_int;
}

/// WebKitGTK, loaded the first time it's asked for. None: it isn't installed (or is too old).
fn api_opt() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        let name = c(LIBRARY);
        // SAFETY: dlopen of a system library by name; the handle is kept for the life of the process.
        let lib = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
        if lib.is_null() {
            return None;
        }
        // SAFETY: the handle is valid; the signatures are WebKitGTK 6.0's.
        unsafe { Api::load(lib) }
    })
    .as_ref()
}

/// WebKitGTK 6.0 is there (the shared window can open).
pub fn available() -> bool {
    api_opt().is_some()
}

fn api() -> &'static Api {
    api_opt().expect("WebKitGTK is loaded before the shared window is built")
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn gbool(b: bool) -> c_int {
    if b {
        glib::ffi::GTRUE
    } else {
        glib::ffi::GFALSE
    }
}

fn ptr_of(o: &impl IsA<glib::Object>) -> P {
    o.as_ref().as_ptr() as P
}

/// `capsule://app/<path>`: a file of the page (bridge::asset), else "not found". Nothing else is ever loaded.
unsafe extern "C" fn serve(req: P, _data: P) {
    let w = api();
    // SAFETY: WebKit hands a live request; the URI string is valid while it is.
    let uri = unsafe { (w.webkit_uri_scheme_request_get_uri)(req) };
    let uri = if uri.is_null() { String::new() } else { unsafe { CStr::from_ptr(uri) }.to_string_lossy().into_owned() };
    match uri.strip_prefix(ORIGIN).and_then(|p| bridge::asset(&format!("/{p}"))) {
        Some((bytes, mime)) => {
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
            let mime = c(mime);
            // SAFETY: the response takes its own reference to the stream; we drop ours (and the response) after.
            unsafe {
                let response = (w.webkit_uri_scheme_response_new)(ptr_of(&stream), bytes.len() as i64);
                (w.webkit_uri_scheme_response_set_content_type)(response, mime.as_ptr());
                (w.webkit_uri_scheme_response_set_status)(response, 200, c"OK".as_ptr());
                (w.webkit_uri_scheme_request_finish_with_response)(req, response);
                glib::gobject_ffi::g_object_unref(response as *mut _);
            }
        }
        None => {
            let e = glib::Error::new(gio::IOErrorEnum::NotFound, "not found");
            let ptr: *const glib::ffi::GError = e.to_glib_none().0;
            // SAFETY: finish_error copies the error.
            unsafe { (w.webkit_uri_scheme_request_finish_error)(req, ptr as P) };
        }
    }
}

/// # Safety
/// `p` is a new (full) reference to a GObject.
unsafe fn object(p: P) -> glib::Object {
    unsafe { from_glib_full(p as *mut glib::gobject_ffi::GObject) }
}

/// A web context that serves the page from `capsule://app/` (secure, so its modules load).
pub fn context() -> glib::Object {
    let w = api();
    let scheme = c(SCHEME);
    // SAFETY: plain constructors and setters on objects we own; the scheme string is copied.
    unsafe {
        let ctx = (w.webkit_web_context_new)();
        let security = (w.webkit_web_context_get_security_manager)(ctx);
        if !security.is_null() {
            (w.webkit_security_manager_register_uri_scheme_as_secure)(security, scheme.as_ptr());
            (w.webkit_security_manager_register_uri_scheme_as_cors_enabled)(security, scheme.as_ptr());
        }
        (w.webkit_web_context_register_uri_scheme)(ctx, scheme.as_ptr(), Some(serve), std::ptr::null_mut(), None);
        object(ctx)
    }
}

/// The view's settings: no developer tools (unless `devtools`), no page access to the clipboard.
pub fn settings(devtools: bool) -> glib::Object {
    let w = api();
    // SAFETY: a new settings object we own.
    unsafe {
        let s = (w.webkit_settings_new)();
        (w.webkit_settings_set_enable_developer_extras)(s, gbool(devtools));
        (w.webkit_settings_set_javascript_can_access_clipboard)(s, gbool(false));
        (w.webkit_settings_set_enable_back_forward_navigation_gestures)(s, gbool(false));
        object(s)
    }
}

/// The page's scripts and styles, and its messages to us.
#[derive(Clone)]
pub struct Content(glib::Object);

impl Content {
    pub fn new() -> Content {
        // SAFETY: a new object we own.
        Content(unsafe { object((api().webkit_user_content_manager_new)()) })
    }

    fn ptr(&self) -> P {
        ptr_of(&self.0)
    }

    /// Run `source` in the top frame before the page's own scripts.
    pub fn add_script(&self, source: &str) {
        let (w, source) = (api(), c(source));
        // SAFETY: the manager keeps its own reference to the script.
        unsafe {
            let script = (w.webkit_user_script_new)(source.as_ptr(), INJECT_TOP_FRAME, INJECT_AT_DOCUMENT_START, std::ptr::null(), std::ptr::null());
            (w.webkit_user_content_manager_add_script)(self.ptr(), script);
            (w.webkit_user_script_unref)(script);
        }
    }

    /// Replace our style sheet with `css`.
    pub fn set_style(&self, css: &str) {
        let (w, css) = (api(), c(css));
        // SAFETY: as above.
        unsafe {
            (w.webkit_user_content_manager_remove_all_style_sheets)(self.ptr());
            let sheet = (w.webkit_user_style_sheet_new)(css.as_ptr(), INJECT_TOP_FRAME, STYLE_LEVEL_USER, std::ptr::null(), std::ptr::null());
            (w.webkit_user_content_manager_add_style_sheet)(self.ptr(), sheet);
            (w.webkit_user_style_sheet_unref)(sheet);
        }
    }

    /// `webkit.messageHandlers.<name>.postMessage(x)`: `f` gets x as text.
    pub fn on_message(&self, name: &str, f: impl Fn(String) + 'static) {
        let cname = c(name);
        // SAFETY: registers a handler by name on our manager.
        unsafe { (api().webkit_user_content_manager_register_script_message_handler)(self.ptr(), cname.as_ptr(), std::ptr::null()) };
        self.0.connect_local(&format!("script-message-received::{name}"), false, move |args| {
            let value = args.get(1).and_then(|v| v.get::<glib::Object>().ok())?;
            // SAFETY: a JSCValue (a GObject) alive for this call; to_string gives a new string we own.
            let text: glib::GString = unsafe { from_glib_full((api().jsc_value_to_string)(ptr_of(&value))) };
            f(text.to_string());
            None
        });
    }
}

/// The view, with its context, content and settings (construct-only properties).
pub fn web_view(context: &glib::Object, content: &Content, settings: &glib::Object) -> gtk::Widget {
    // SAFETY: the registered type of WebKitWebView.
    let kind: glib::Type = unsafe { from_glib((api().webkit_web_view_get_type)()) };
    glib::Object::builder_with_type(kind)
        .property("web-context", context)
        .property("user-content-manager", &content.0)
        .property("settings", settings)
        .build()
        .downcast::<gtk::Widget>()
        .expect("WebKitWebView is a widget")
}

pub fn load_uri(view: &gtk::Widget, uri: &str) {
    let uri = c(uri);
    // SAFETY: our view; the string is copied.
    unsafe { (api().webkit_web_view_load_uri)(ptr_of(view), uri.as_ptr()) };
}

/// Run `script` in the page (its result is not needed).
pub fn run_js(view: &gtk::Widget, script: &str) {
    let s = c(script);
    // SAFETY: our view; the script is copied; no callback.
    unsafe {
        (api().webkit_web_view_evaluate_javascript)(
            ptr_of(view),
            s.as_ptr(),
            s.as_bytes().len() as isize,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
        )
    };
}

pub fn set_background(view: &gtk::Widget, rgba: &gdk::RGBA) {
    let color: *const gdk::ffi::GdkRGBA = rgba.to_glib_none().0;
    // SAFETY: our view; the colour is copied.
    unsafe { (api().webkit_web_view_set_background_color)(ptr_of(view), color) };
}

/// Every navigation and new-window request: `allow(uri, new_window)` says whether it may load here.
pub fn on_navigation(view: &gtk::Widget, allow: impl Fn(&str, bool) -> bool + 'static) {
    view.connect_local("decide-policy", false, move |args| Some(decide(args, &allow).to_value()));
}

/// true: handled here (ignored); false: WebKit decides.
fn decide(args: &[glib::Value], allow: &dyn Fn(&str, bool) -> bool) -> bool {
    let w = api();
    let Some(decision) = args.get(1).and_then(|v| v.get::<glib::Object>().ok()) else { return false };
    let Some(kind) = args.get(2) else { return false };
    // SAFETY: reads the enum of a live signal argument.
    let kind = unsafe { glib::gobject_ffi::g_value_get_enum(kind.to_glib_none().0) };
    // SAFETY: the type of navigation decisions.
    let nav: glib::Type = unsafe { from_glib((w.webkit_navigation_policy_decision_get_type)()) };
    if kind > NEW_WINDOW_ACTION || !decision.type_().is_a(nav) {
        return false; // responses: as WebKit decides
    }
    // SAFETY: a live navigation decision; the action and request it returns are owned by it, the URI too.
    let uri = unsafe {
        let action = (w.webkit_navigation_policy_decision_get_navigation_action)(ptr_of(&decision));
        let request = if action.is_null() { std::ptr::null_mut() } else { (w.webkit_navigation_action_get_request)(action) };
        let uri = if request.is_null() { std::ptr::null() } else { (w.webkit_uri_request_get_uri)(request) };
        if uri.is_null() {
            String::new()
        } else {
            CStr::from_ptr(uri).to_string_lossy().into_owned()
        }
    };
    if allow(&uri, kind == NEW_WINDOW_ACTION) {
        return false;
    }
    // SAFETY: as above.
    unsafe { (w.webkit_policy_decision_ignore)(ptr_of(&decision)) };
    true
}

/// The right-click menu: `show(in_text)` says whether WebKit's menu shows (`in_text`: an editable field or a
/// selection).
pub fn on_context_menu(view: &gtk::Widget, show: impl Fn(bool) -> bool + 'static) {
    view.connect_local("context-menu", false, move |args| {
        let Some(hit) = args.get(2).and_then(|v| v.get::<glib::Object>().ok()) else { return Some(false.to_value()) };
        let w = api();
        // SAFETY: a live hit-test result.
        let in_text = unsafe {
            (w.webkit_hit_test_result_context_is_editable)(ptr_of(&hit)) != 0 || (w.webkit_hit_test_result_context_is_selection)(ptr_of(&hit)) != 0
        };
        Some((!show(in_text)).to_value())
    });
}
