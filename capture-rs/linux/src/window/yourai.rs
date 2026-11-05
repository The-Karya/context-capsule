// context-capsule — linux: Your AI in the window — Make today's capsules (Claude with the request typed in,
// another app with it on the clipboard, or Copy my day), Copy my day and Save from clipboard, a request for Your
// AI, and connecting Claude (through its .mcpb or Claude Code) or another AI app when a request needs it. The
// program runs on worker threads (`crate::ai`); results come back on GTK's thread.

use super::widgets::{self, label, vbox};
use super::Ctx;
use crate::ai::{self, Day, Told};
use crate::{connect, reminder, ui};
use adw::prelude::*;
use context_capsule_core::anyai::{self, AiClient, YourAi};
use context_capsule_core::reminder::DAY_PROMPT;
use context_capsule_core::window::CANCEL;
use context_capsule_core::Msg;
use gtk::{gio, glib};
use std::rc::Rc;

/// Run `work` off the GTK thread, then `then` on it.
fn in_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, then: impl FnOnce(T) + 'static) {
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(work).await {
            Ok(v) => then(v),
            Err(_) => ai::log("any AI: a task stopped"),
        }
    });
}

impl Ctx {
    /// A message to dismiss, over the window (brought up first if it was hidden).
    pub fn alert(&self, title: &str, body: &str) {
        if !self.win.is_visible() {
            self.win.present();
        }
        widgets::message(&self.win, title, body);
    }

    /// "Make today's capsules", the way Your AI works; stops contextualizing first (unless a voice note is
    /// listening).
    pub fn make_capsules(self: &Rc<Self>) {
        let s = self.status.borrow().clone();
        if s.recording && !s.listening {
            ai::log("stop: make today's capsules");
            self.send(Msg::ToggleRecord);
        }
        match ai::state().kind() {
            k if k.uses_copy() => self.copy_my_day(),
            YourAi::Claude => self.ask_claude(DAY_PROMPT),
            YourAi::App(_) => self.open_prompt(DAY_PROMPT, ai::WHY_DAY, ai::AGAIN_DAY),
            YourAi::Website(_) | YourAi::Other => self.copy_my_day(),
        }
    }

    /// A request for Your AI. Another app: connect it first, or the request goes on the clipboard. Claude, a
    /// website or any other AI: Claude, as before (these need Context Capsule's tools).
    pub fn open_prompt(self: &Rc<Self>, prompt: &str, why: &str, again: &str) {
        if !ai::state().loaded {
            // Just started (a capsule opened at launch): which app it is comes with the list.
            let (cx, prompt, why, again) = (self.clone(), prompt.to_string(), why.to_string(), again.to_string());
            in_background(ai::ensure_loaded, move |()| cx.open_prompt_now(&prompt, &why, &again));
            return;
        }
        self.open_prompt_now(prompt, why, again);
    }

    fn open_prompt_now(self: &Rc<Self>, prompt: &str, why: &str, again: &str) {
        let s = ai::state();
        let client = match s.kind() {
            YourAi::App(id) => s.client(&id).cloned(),
            _ => None,
        };
        let Some(c) = client else { return self.ask_claude(prompt) };
        if c.connected {
            self.copy_text(prompt);
            self.toast(&anyai::copied_for(&c.name, false));
            return;
        }
        let (title, body) = anyai::connect_first(&c.name, why, again);
        if !self.win.is_visible() {
            self.win.present();
        }
        let d = adw::MessageDialog::new(Some(&self.win), Some(&title), Some(&body));
        d.add_response("cancel", CANCEL);
        d.add_response("connect", anyai::CONNECT);
        d.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("connect"));
        d.set_close_response("cancel");
        let cx = self.clone();
        d.connect_response(None, move |_, response| {
            if response == "connect" {
                cx.connect_ai(&c);
            }
        });
        d.present();
    }

    /// Copy my day: today's work as one message on the clipboard; the website opens when Your AI is one.
    pub fn copy_my_day(self: &Rc<Self>) {
        if !ai::begin_work() {
            return;
        }
        let cx = self.clone();
        glib::spawn_future_local(async move {
            let day = gio::spawn_blocking(ai::day).await;
            ai::end_work();
            match day {
                Ok(Day::Copy { text, chars, url }) => {
                    cx.copy_text(&text);
                    if let Some(url) = url {
                        if !cx.open_uri(url) {
                            cx.toast(ui::CANT_OPEN);
                        }
                    }
                    ai::day_copied(chars);
                    cx.toast(anyai::PASTE_LINE);
                }
                Ok(Day::Nothing) => cx.toast(anyai::NOTHING_TODAY),
                Ok(Day::Failed(why)) => cx.alert(anyai::COPY_FAILED, &why),
                Err(_) => cx.alert(anyai::COPY_FAILED, ai::TRY_AGAIN),
            }
        });
    }

    /// Save from clipboard: the capsules in the AI's reply, then what was saved and what wasn't.
    pub fn save_from_clipboard(self: &Rc<Self>) {
        if ai::state().working {
            return;
        }
        let cx = self.clone();
        glib::spawn_future_local(async move {
            let read = cx.win.clipboard().read_text_future().await;
            let mut text = read.ok().flatten().map(|t| t.to_string()).unwrap_or_default();
            if text.trim().is_empty() && !cx.win.is_active() {
                // Not in front yet (Wayland hands the clipboard to the window in front): the clipboard tools.
                text = gio::spawn_blocking(reminder::paste_text).await.ok().flatten().unwrap_or_default();
            }
            if text.trim().is_empty() {
                cx.alert(anyai::CLIPBOARD_EMPTY, anyai::CLIPBOARD_EMPTY_BODY);
                return;
            }
            if !ai::begin_work() {
                return;
            }
            let saved = gio::spawn_blocking(move || ai::save(&text)).await;
            ai::end_work();
            match saved {
                Ok(Ok((title, body))) => cx.alert(&title, &body),
                Ok(Err(why)) => cx.alert(anyai::SAVE_FAILED, &why),
                Err(_) => cx.alert(anyai::SAVE_FAILED, ai::TRY_AGAIN),
            }
        });
    }

    /// Connect: Claude through its own flow, any other app through its MCP settings.
    pub fn connect_ai(self: &Rc<Self>, c: &AiClient) {
        if c.is_extension() {
            self.connect_claude();
            return;
        }
        let (c, cx) = (c.clone(), self.clone());
        in_background(move || ai::connect(&c), move |told| cx.tell(told));
    }

    /// A note shows under the list by itself (the state has it); a failure is a message.
    fn tell(&self, told: Told) {
        if let Told::Alert(title, body) = told {
            self.alert(&title, &body);
        }
    }

    /// Claude: hand context-capsule.mcpb to a Claude app if one takes it; else the Claude Code command (with the
    /// context-capsule-mcp program when there is one).
    pub fn connect_claude(self: &Rc<Self>) {
        let exe = std::env::current_exe().ok();
        if let Some(ext) = exe.as_deref().map(connect::extension_next_to).filter(|p| p.is_file()) {
            let (kind, _) = gio::content_type_guess(Some(&ext), None);
            let claude = gio::AppInfo::default_for_type(&kind, false)
                .filter(|a| connect::is_claude_app(&format!("{} {}", a.id().unwrap_or_default(), a.executable().to_string_lossy())));
            if let Some(app) = claude {
                if app.launch(&[gio::File::for_path(&ext)], Some(&self.launch_context())).is_ok() {
                    ai::set_note(anyai::CLAUDE_INSTALL_NOTE);
                    return;
                }
            }
        }
        let cx = self.clone();
        in_background(ai::launch, move |launch| {
            let entry = exe.as_deref().and_then(|e| connect::mcp_entry(e, &|p| p.is_file()));
            let (command, node) = connect::claude_code_command_for(launch.as_ref(), entry.as_deref());
            cx.claude_code_dialog(command, if node { connect::CONNECT_AFTER } else { connect::CONNECT_AFTER_PROGRAM });
        });
    }

    fn claude_code_dialog(self: &Rc<Self>, command: String, after: &str) {
        let d = adw::MessageDialog::new(Some(&self.win), Some(connect::CONNECT_TITLE), Some(connect::CONNECT_BODY));
        let extra = vbox(10);
        let cmd = label(&command, &["command"]);
        cmd.set_selectable(true);
        extra.append(&cmd);
        extra.append(&label(after, &["dim-label", "caption"]));
        d.set_extra_child(Some(&extra));
        d.add_response("close", connect::CLOSE);
        d.add_response("copy", connect::COPY);
        d.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("copy"));
        d.set_close_response("close");
        let cx = self.clone();
        d.connect_response(None, move |_, response| {
            if response == "copy" {
                cx.copy_text(&command);
                cx.toast(connect::COPIED);
            }
        });
        d.present();
    }
}
