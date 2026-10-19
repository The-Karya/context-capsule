// context-capsule — windows: the focused window's text through UI Automation — a cached walk of the
// control view within the core's node / time / depth limits. Password fields are never read.
//! Roles mirror the macOS AX roles: text, edits, documents and combo boxes carry a value; links,
//! headers, list items and grid cells are labels; everything else only holds children.

use context_capsule_core::NodeKind;

// UIA control type ids (UIAutomationClient.h).
pub const COMBO_BOX: i32 = 50003;
pub const EDIT: i32 = 50004;
pub const HYPERLINK: i32 = 50005;
pub const LIST_ITEM: i32 = 50007;
pub const TEXT: i32 = 50020;
pub const DATA_ITEM: i32 = 50029;
pub const DOCUMENT: i32 = 50030;
pub const HEADER: i32 = 50034;
pub const HEADER_ITEM: i32 = 50035;

/// A control's role for `UiText::assemble`. A password field is `Secure` whatever its type.
pub fn kind_for(control_type: i32, is_password: bool) -> NodeKind {
    if is_password {
        return NodeKind::Secure;
    }
    match control_type {
        TEXT | EDIT | DOCUMENT | COMBO_BOX => NodeKind::Value,
        HYPERLINK | HEADER | HEADER_ITEM | DATA_ITEM | LIST_ITEM => NodeKind::Label,
        _ => NodeKind::Other,
    }
}

/// Edits and documents with no value and no children (Notepad, Word) keep their text in TextPattern.
pub fn wants_text_pattern(control_type: i32, value: &str, has_children: bool) -> bool {
    matches!(control_type, EDIT | DOCUMENT) && value.trim().is_empty() && !has_children
}

#[cfg(windows)]
pub use win::Uia;

#[cfg(windows)]
mod win {
    use super::*;
    use context_capsule_core::{TextNode, UiText};
    use std::time::Instant;
    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern, UIA_ControlTypePropertyId,
        UIA_HelpTextPropertyId, UIA_IsPasswordPropertyId, UIA_NamePropertyId, UIA_TextPatternId, UIA_ValuePatternId,
    };

    /// One UI Automation client, made on the engine thread (COM, multithreaded apartment).
    pub struct Uia {
        automation: IUIAutomation,
        cache: IUIAutomationCacheRequest,
        walker: IUIAutomationTreeWalker,
    }

    struct Walk<'a> {
        uia: &'a Uia,
        nodes: Vec<TextNode>,
        deadline: Instant,
    }

    impl Uia {
        pub fn new() -> Option<Uia> {
            // SAFETY: COM object creation and setup calls on this thread's apartment.
            unsafe {
                let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                    .or_else(|_| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER))
                    .ok()?;
                // A frozen app must not stall sampling.
                if let Ok(a2) = automation.cast::<IUIAutomation2>() {
                    let _ = a2.SetConnectionTimeout(1000);
                    let _ = a2.SetTransactionTimeout(250);
                }
                let cache = automation.CreateCacheRequest().ok()?;
                for property in [UIA_ControlTypePropertyId, UIA_IsPasswordPropertyId, UIA_NamePropertyId, UIA_HelpTextPropertyId] {
                    cache.AddProperty(property).ok()?;
                }
                let walker = automation.ControlViewWalker().ok()?;
                Some(Uia { automation, cache, walker })
            }
        }

        /// The window's text, assembled by the core. None if UI Automation cannot reach the window.
        pub fn text(&self, hwnd: HWND) -> Option<String> {
            // SAFETY: a COM call; a dead window just fails.
            let root = unsafe { self.automation.ElementFromHandleBuildCache(hwnd, &self.cache) }.ok()?;
            let mut walk = Walk { uia: self, nodes: vec![], deadline: Instant::now() + UiText::BUDGET };
            walk.visit(&root, 0);
            Some(UiText::assemble(&walk.nodes, UiText::MAX_CHARS))
        }
    }

    impl Walk<'_> {
        fn done(&self) -> bool {
            self.nodes.len() >= UiText::MAX_NODES || Instant::now() >= self.deadline
        }

        /// Depth first, parents before children.
        fn visit(&mut self, element: &IUIAutomationElement, depth: usize) {
            if self.done() {
                return;
            }
            // SAFETY: reads of cached properties (no cross-process call).
            let (control_type, is_password, name, help) = unsafe {
                (
                    element.CachedControlType().map(|c| c.0).unwrap_or(0),
                    element.CachedIsPassword().map(|b| b.as_bool()).unwrap_or(false),
                    element.CachedName().map(|s| s.to_string()).unwrap_or_default(),
                    element.CachedHelpText().map(|s| s.to_string()).unwrap_or_default(),
                )
            };
            let kind = kind_for(control_type, is_password);
            if kind == NodeKind::Secure {
                // Noted so assembly skips it; its value and children are never read.
                self.nodes.push(TextNode { depth, kind, ..Default::default() });
                return;
            }
            let value = if matches!(control_type, EDIT | DOCUMENT | COMBO_BOX) { self.value(element) } else { String::new() };
            let index = self.nodes.len();
            self.nodes.push(TextNode { depth, kind, value, title: name, desc: help });
            if depth >= UiText::MAX_DEPTH {
                return;
            }

            let uia = self.uia;
            // SAFETY: COM tree-walk calls; each returns a new element or fails at the end.
            let mut child = unsafe { uia.walker.GetFirstChildElementBuildCache(element, &uia.cache) }.ok();
            if child.is_none() && wants_text_pattern(control_type, &self.nodes[index].value, false) {
                self.nodes[index].value = self.document_text(element);
            }
            while let Some(c) = child {
                if self.done() {
                    return;
                }
                self.visit(&c, depth + 1);
                // SAFETY: as above.
                child = unsafe { uia.walker.GetNextSiblingElementBuildCache(&c, &uia.cache) }.ok();
            }
        }

        /// The ValuePattern value (only ever asked of non-password fields).
        fn value(&self, element: &IUIAutomationElement) -> String {
            // SAFETY: COM calls on a live element.
            unsafe {
                element
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .and_then(|p| p.CurrentValue())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
        }

        /// The whole text of a leaf document or edit, capped like the rest.
        fn document_text(&self, element: &IUIAutomationElement) -> String {
            // SAFETY: COM calls on a live element.
            unsafe {
                element
                    .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                    .and_then(|p| p.DocumentRange())
                    .and_then(|r| r.GetText(UiText::MAX_CHARS as i32))
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_mirror_ax() {
        for t in [TEXT, EDIT, DOCUMENT, COMBO_BOX] {
            assert_eq!(kind_for(t, false), NodeKind::Value, "{t}");
        }
        for t in [HYPERLINK, HEADER, HEADER_ITEM, DATA_ITEM, LIST_ITEM] {
            assert_eq!(kind_for(t, false), NodeKind::Label, "{t}");
        }
        assert_eq!(kind_for(50000, false), NodeKind::Other, "buttons hold no text of their own");
        assert_eq!(kind_for(50033, false), NodeKind::Other, "panes only hold children");
    }

    #[test]
    fn password_is_secure_whatever_the_type() {
        assert_eq!(kind_for(EDIT, true), NodeKind::Secure);
        assert_eq!(kind_for(TEXT, true), NodeKind::Secure);
        assert_eq!(kind_for(50033, true), NodeKind::Secure);
    }

    #[test]
    fn text_pattern_only_for_leaf_documents() {
        assert!(wants_text_pattern(DOCUMENT, "", false));
        assert!(wants_text_pattern(EDIT, "  ", false));
        assert!(!wants_text_pattern(DOCUMENT, "", true), "a browser page reads its children");
        assert!(!wants_text_pattern(EDIT, "typed", false), "the value is enough");
        assert!(!wants_text_pattern(TEXT, "", false));
    }

    #[test]
    fn secure_subtree_is_skipped_by_assembly() {
        use context_capsule_core::{TextNode, UiText};
        let n = |depth, kind, title: &str| TextNode { depth, kind, title: title.into(), ..Default::default() };
        let nodes = vec![
            n(0, kind_for(50032, false), "Sign in"),
            n(1, kind_for(TEXT, false), "Email"),
            n(1, kind_for(EDIT, true), "Password"),
            n(2, kind_for(TEXT, false), "hunter2"),
            n(1, kind_for(HYPERLINK, false), "Forgot password?"),
        ];
        assert_eq!(UiText::assemble(&nodes, 8000), "Email\nForgot password?");
    }
}
