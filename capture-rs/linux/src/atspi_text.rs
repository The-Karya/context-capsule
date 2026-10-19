// context-capsule — linux: the active window's text from its AT-SPI tree — depth first, within the
// core's node, depth and time limits. Password fields (and anything inside them) are never read.

use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::text::TextProxy;
use atspi::{Interface, ObjectRefOwned, Role, State};
use context_capsule_core::{NodeKind, TextNode, UiText};
use std::time::Instant;
use zbus::proxy::CacheProperties;

/// What a role contributes. Text-bearing roles give their text (`Value`); headings, links, cells and
/// list items their name (`Label`); password fields are `Secure`.
pub fn node_kind(role: Role) -> NodeKind {
    match role {
        Role::PasswordText => NodeKind::Secure,
        Role::Text
        | Role::Paragraph
        | Role::Entry
        | Role::Label
        | Role::Static
        | Role::DocumentText
        | Role::DocumentWeb
        | Role::DocumentEmail
        | Role::Terminal
        | Role::ComboBox
        | Role::Section
        | Role::Article
        | Role::BlockQuote
        | Role::Caption
        | Role::Comment
        | Role::Definition
        | Role::DescriptionTerm
        | Role::DescriptionValue
        | Role::Footnote
        | Role::Mark
        | Role::ContentInsertion
        | Role::ContentDeletion
        | Role::Suggestion => NodeKind::Value,
        Role::Heading | Role::Link | Role::TableCell | Role::ListItem => NodeKind::Label,
        _ => NodeKind::Other,
    }
}

/// AT-SPI puts U+FFFC where a child object sits inside its parent's text; the child is read on its own.
pub fn strip_embedded(s: &str) -> String {
    s.replace('\u{FFFC}', " ")
}

/// Which part of a long text to fetch: a terminal's end (the newest output), else its start.
pub fn text_range(role: Role, count: i32, max: i32) -> (i32, i32) {
    if count <= max {
        (0, count)
    } else if role == Role::Terminal {
        (count - max, count)
    } else {
        (0, max)
    }
}

/// Walk the window's tree and assemble its text. None if the walk could not start.
pub async fn read(conn: &zbus::Connection, window: &ObjectRefOwned) -> Option<String> {
    let start = Instant::now();
    let mut nodes: Vec<TextNode> = vec![];
    // Depth first, parents before children: a stack of (object, depth), children pushed in reverse.
    let mut stack: Vec<(ObjectRefOwned, usize)> = vec![(window.clone(), 0)];
    let mut first = true;
    while let Some((obj, depth)) = stack.pop() {
        if nodes.len() >= UiText::MAX_NODES || start.elapsed() >= UiText::BUDGET {
            break;
        }
        let Ok(proxy) = obj.as_accessible_proxy(conn).await else {
            if first {
                return None;
            }
            continue;
        };
        let (role, state, children) = futures_util::join!(proxy.get_role(), proxy.get_state(), proxy.get_children());
        if first && role.is_err() {
            return None;
        }
        first = false;
        let role = role.unwrap_or(Role::Invalid);
        let state = state.unwrap_or(atspi::StateSet::empty());
        // Hidden parts (background tabs, collapsed panes) are not on screen: skip them.
        if depth > 0 && !state.contains(State::Showing) {
            continue;
        }
        let kind = node_kind(role);
        let mut node = TextNode { depth, kind, ..Default::default() };
        if matches!(kind, NodeKind::Value | NodeKind::Label) {
            node.title = proxy.name().await.unwrap_or_default();
            node.value = text_of(conn, &proxy, &obj, role).await.unwrap_or_default();
        }
        nodes.push(node);
        if kind == NodeKind::Secure || depth + 1 > UiText::MAX_DEPTH {
            continue; // never inside a password field
        }
        if let Ok(children) = children {
            for child in children.into_iter().rev() {
                if !child.is_null() {
                    stack.push((child, depth + 1));
                }
            }
        }
    }
    Some(UiText::assemble(&nodes, UiText::MAX_CHARS))
}

async fn text_of(
    conn: &zbus::Connection,
    proxy: &AccessibleProxy<'_>,
    obj: &ObjectRefOwned,
    role: Role,
) -> Option<String> {
    let interfaces = proxy.get_interfaces().await.ok()?;
    if !interfaces.contains(Interface::Text) {
        return None;
    }
    let text = TextProxy::builder(conn)
        .destination(obj.name()?.clone())
        .ok()?
        .path(obj.path().clone())
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let count = text.character_count().await.ok()?;
    if count <= 0 {
        return None;
    }
    let (from, to) = text_range(role, count, UiText::MAX_CHARS as i32);
    Some(strip_embedded(&text.get_text(from, to).await.ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        for r in
            [Role::Text, Role::Paragraph, Role::Entry, Role::Label, Role::Static, Role::DocumentText, Role::Terminal]
        {
            assert_eq!(node_kind(r), NodeKind::Value, "{r:?}");
        }
        for r in [Role::Heading, Role::Link, Role::TableCell, Role::ListItem] {
            assert_eq!(node_kind(r), NodeKind::Label, "{r:?}");
        }
        assert_eq!(node_kind(Role::PasswordText), NodeKind::Secure);
        for r in [Role::Button, Role::Frame, Role::Panel, Role::Image, Role::ScrollBar] {
            assert_eq!(node_kind(r), NodeKind::Other, "{r:?}");
        }
    }

    #[test]
    fn password_subtree_never_read() {
        let n = |depth, role, value: &str| TextNode {
            depth,
            kind: node_kind(role),
            value: value.into(),
            ..Default::default()
        };
        let nodes = [
            n(0, Role::Frame, ""),
            n(1, Role::Paragraph, "Sign in"),
            n(1, Role::PasswordText, "hunter2"),
            n(2, Role::Text, "hunter2 again"),
            n(1, Role::Label, "Forgot it?"),
        ];
        assert_eq!(UiText::assemble(&nodes, UiText::MAX_CHARS), "Sign in\nForgot it?");
    }

    #[test]
    fn embedded_objects_and_ranges() {
        assert_eq!(strip_embedded("Read \u{FFFC} now"), "Read   now");
        assert_eq!(text_range(Role::Paragraph, 100, 8000), (0, 100));
        assert_eq!(text_range(Role::Paragraph, 9000, 8000), (0, 8000));
        assert_eq!(text_range(Role::Terminal, 9000, 8000), (1000, 9000), "a terminal's newest lines");
    }
}
