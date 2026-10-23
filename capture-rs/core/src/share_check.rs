// context-capsule — core: "Before you share" — what in a capsule might be private (people, emails, phone numbers,
// private links), found with plain rules (no AI), and a copy with the checked items replaced by `[removed]`.
// The same rules as ui/js/core/sharecheck.js and the Mac (SPEC.md "Check before sharing"); spec/share-check.json
// holds the cases all must pass. Capitalised words, @handles, money, file paths and public links are not flagged.
//! Only the body is searched (a leading `---` front matter block is skipped); `remove` changes the whole file.

use regex::Regex;
use std::sync::OnceLock;

/// The dialog's title, its line, and its buttons.
pub const TITLE: &str = "Before you share";
pub const LINE: &str = "Ticked items are replaced with [removed] in the copy you share. Your capsule stays the same.";
pub const CANCEL: &str = "Cancel";
pub const SHARE: &str = "Share";

/// One group of things that might be private.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    People,
    Emails,
    Phones,
    Links,
}

impl Kind {
    /// In the order they are shown.
    pub const ALL: [Kind; 4] = [Kind::People, Kind::Emails, Kind::Phones, Kind::Links];

    /// The group's heading.
    pub fn title(self) -> &'static str {
        match self {
            Kind::People => "People",
            Kind::Emails => "Emails",
            Kind::Phones => "Phone numbers",
            Kind::Links => "Private links",
        }
    }

    /// Its key in spec/share-check.json.
    pub fn key(self) -> &'static str {
        match self {
            Kind::People => "people",
            Kind::Emails => "emails",
            Kind::Phones => "phones",
            Kind::Links => "links",
        }
    }
}

/// What replaces each checked item in the shared copy.
pub const REMOVED: &str = "[removed]";

/// Everything found, per group (each item once, in order of first appearance in the body).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub people: Vec<String>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub links: Vec<String>,
}

impl Found {
    pub fn get(&self, kind: Kind) -> &[String] {
        match kind {
            Kind::People => &self.people,
            Kind::Emails => &self.emails,
            Kind::Phones => &self.phones,
            Kind::Links => &self.links,
        }
    }

    /// Nothing found: share straight away.
    pub fn is_empty(&self) -> bool {
        Kind::ALL.iter().all(|k| self.get(*k).is_empty())
    }

    /// The groups with something in them, in order.
    pub fn groups(&self) -> Vec<(Kind, &[String])> {
        Kind::ALL.iter().map(|k| (*k, self.get(*k))).filter(|(_, items)| !items.is_empty()).collect()
    }

    /// Every item, group by group (the order the check boxes are shown in).
    pub fn items(&self) -> Vec<(Kind, String)> {
        self.groups().into_iter().flat_map(|(k, items)| items.iter().map(move |i| (k, i.clone()))).collect()
    }
}

/// A line terminator as JS regexes see them (for `^` and `$` in multi-line mode).
fn is_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Where the body starts (byte offset): after a leading `---` front matter block's closing `---` line (the
/// newline after it is part of the body), else 0. As the JS `body`.
pub fn body_start(text: &str) -> usize {
    let skip = if text.starts_with("---\r\n") {
        5
    } else if text.starts_with("---\n") {
        4
    } else {
        return 0;
    };
    let rest = &text[skip..];
    // `^---[ \t]*\r?$`, multi-line: the first line that is `---` (then spaces or tabs).
    let mut line_start = true;
    for (i, c) in rest.char_indices() {
        if line_start && rest[i..].starts_with("---") {
            let mut j = i + 3;
            while rest[j..].starts_with([' ', '\t']) {
                j += 1;
            }
            let end_ok = |at: usize| rest[at..].chars().next().is_none_or(is_terminator);
            let end = if rest[j..].starts_with('\r') && end_ok(j + 1) {
                Some(j + 1)
            } else if end_ok(j) {
                Some(j)
            } else {
                None
            };
            if let Some(end) = end {
                return skip + end;
            }
        }
        line_start = is_terminator(c);
    }
    0
}

/// Find what might be private in a capsule's body.
pub fn find(text: &str) -> Found {
    let body = &text[body_start(text)..];
    let mut work: Vec<char> = body.chars().collect();

    // Links, then emails; each blanked out (same length) so later rules can't match inside them.
    let links = find_links(&work);
    let link_texts: Vec<String> = links.iter().map(|(a, b)| text_of(&work, *a, *b)).collect();
    blank(&mut work, &links);
    let emails = find_emails(&work);
    let email_texts: Vec<String> = emails.iter().map(|(a, b)| text_of(&work, *a, *b)).collect();
    blank(&mut work, &emails);
    let phones = find_phones(&work);

    Found {
        people: ordered(body, bullet_names(body)),
        emails: ordered(body, email_texts),
        phones: ordered(body, phones),
        links: ordered(body, link_texts.into_iter().filter(|l| is_private_link(l)).collect()),
    }
}

/// A copy of `text` with every occurrence of each of `items` (longest first) replaced by `[removed]`. One pass,
/// left to right, so a short item never matches inside a `[removed]` put there for a longer one. The whole file,
/// front matter too (a name in the title is just as private).
pub fn remove(text: &str, items: &[String]) -> String {
    let mut items: Vec<&str> = items.iter().map(String::as_str).filter(|i| !i.is_empty()).collect();
    items.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(b.cmp(a)));
    items.dedup();
    if items.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'scan: while i < text.len() {
        for item in &items {
            if text[i..].starts_with(item) {
                out.push_str(REMOVED);
                i += item.len();
                continue 'scan;
            }
        }
        let c = text[i..].chars().next().expect("inside the text");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Each item once, in order of first appearance in the body (not found: last, in the order given).
fn ordered(body: &str, items: Vec<String>) -> Vec<String> {
    let mut seen: Vec<(usize, String)> = vec![];
    for item in items {
        if seen.iter().any(|(_, s)| *s == item) {
            continue;
        }
        let at = body.find(&item).unwrap_or(usize::MAX);
        seen.push((at, item));
    }
    seen.sort_by_key(|(at, _)| *at);
    seen.into_iter().map(|(_, s)| s).collect()
}

fn blank(work: &mut [char], spans: &[(usize, usize)]) {
    for (a, b) in spans {
        for c in &mut work[*a..*b] {
            *c = ' ';
        }
    }
}

fn starts_with(work: &[char], at: usize, s: &str) -> bool {
    (at..).zip(s.chars()).all(|(i, c)| work.get(i) == Some(&c))
}

fn text_of(work: &[char], a: usize, b: usize) -> String {
    work[a..b].iter().collect()
}

/// `\s` as JS sees it.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `https?://[^\s)>\]]+`, trailing `. , ; :` dropped.
fn find_links(work: &[char]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < work.len() {
        let scheme = if starts_with(work, i, "https://") {
            8
        } else if starts_with(work, i, "http://") {
            7
        } else {
            0
        };
        let stops = |c: char| is_space(c) || matches!(c, ')' | '>' | ']');
        if scheme == 0 || work.get(i + scheme).is_none_or(|c| stops(*c)) {
            i += 1;
            continue;
        }
        let mut j = i + scheme;
        while j < work.len() && !stops(work[j]) {
            j += 1;
        }
        let mut end = j;
        while end > i && matches!(work[end - 1], '.' | ',' | ';' | ':') {
            end -= 1;
        }
        if end > i {
            out.push((i, end));
        }
        i = j;
    }
    out
}

fn is_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

fn is_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
}

/// `[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}`, leftmost, greedy, as a regex finds them one after another.
fn find_emails(work: &[char]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = vec![];
    let mut from = 0; // where the search goes on from (the end of the last match)
    for at in 0..work.len() {
        if work[at] != '@' || at < from {
            continue;
        }
        let mut a = at;
        while a > from && is_local(work[a - 1]) {
            a -= 1;
        }
        if a == at {
            continue;
        }
        let mut run = at + 1;
        while run < work.len() && is_domain(work[run]) {
            run += 1;
        }
        // The last `.` in the domain (not its first character) followed by two or more letters.
        let dot = (at + 2..run).rev().find(|&k| {
            work[k] == '.' && work.get(k + 1).is_some_and(char::is_ascii_alphabetic) && work.get(k + 2).is_some_and(char::is_ascii_alphabetic)
        });
        if let Some(k) = dot {
            let mut end = k + 1;
            while end < work.len() && work[end].is_ascii_alphabetic() {
                end += 1;
            }
            out.push((a, end));
            from = end;
        }
    }
    out
}

/// `\w` as the Mac's regexes see it (`\p{Alphabetic}\p{M}\p{Nd}\p{Pc}`, ZWNJ, ZWJ).
fn is_word(c: char) -> bool {
    static WORD: OnceLock<Regex> = OnceLock::new();
    if c.is_alphabetic() {
        return true;
    }
    if c.is_ascii() {
        return c.is_ascii_digit() || c == '_';
    }
    let re = WORD.get_or_init(|| Regex::new(r"^[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}\x{200C}\x{200D}]$").expect("word"));
    let mut buf = [0u8; 4];
    re.is_match(c.encode_utf8(&mut buf))
}

fn is_phone_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '(' | ')')
}

/// `dddd-dd-dd` somewhere in it: a date, not a phone.
fn has_date(s: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    let shape = "dddd-dd-dd";
    s.windows(shape.len()).any(|w| {
        w.iter().zip(shape.chars()).all(|(c, want)| if want == 'd' { c.is_ascii_digit() } else { *c == want })
    })
}

/// `(?<![\w+(])(?:\+|\()?\d[\d .()\-]*\d(?![\w])`, as a regex finds them one after another; then 8–15 digits and
/// no date in it.
fn find_phones(work: &[char]) -> Vec<String> {
    let mut out = vec![];
    let mut p = 0;
    while p < work.len() {
        if p > 0 && (is_word(work[p - 1]) || matches!(work[p - 1], '+' | '(')) {
            p += 1;
            continue;
        }
        let d = if matches!(work[p], '+' | '(') { p + 1 } else { p };
        if !work.get(d).is_some_and(char::is_ascii_digit) {
            p += 1;
            continue;
        }
        let mut run = d + 1;
        while run < work.len() && is_phone_char(work[run]) {
            run += 1;
        }
        // The longest end: a digit after the first, not followed by a word character.
        let last = (d + 1..run).rev().find(|&k| work[k].is_ascii_digit() && !work.get(k + 1).is_some_and(|c| is_word(*c)));
        let Some(last) = last else {
            p += 1;
            continue;
        };
        let found = text_of(work, p, last + 1);
        let digits = found.chars().filter(char::is_ascii_digit).count();
        if (8..=15).contains(&digits) && !has_date(&found) {
            out.push(found);
        }
        p = last + 1;
    }
    out
}

// ---------- links that look private ----------

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("share_check regex"))
}

/// True when a link looks private: a token, key or id in its query, a local or internal host, a password in it,
/// or a share link to a private document, meeting or file. Ordinary public links (a blog, a homepage) are not.
pub fn is_private_link(url: &str) -> bool {
    static URL: OnceLock<Regex> = OnceLock::new();
    static INTERNAL: OnceLock<Regex> = OnceLock::new();
    static PORT: OnceLock<Regex> = OnceLock::new();
    static SECRET: OnceLock<Regex> = OnceLock::new();
    static RANDOM: OnceLock<Regex> = OnceLock::new();
    let Some(m) = re(&URL, r"(?i)^https?://([^/?#]*)([^?#]*)(?:\?([^#]*))?").captures(url) else { return false };
    let host = m.get(1).map_or("", |h| h.as_str());
    if host.contains('@') {
        return true; // user:password@host
    }
    let host = re(&PORT, r":[0-9]+$").replace(host, "").to_lowercase();
    let path = m.get(2).map_or("", |p| p.as_str());
    let path = if path.is_empty() { "/" } else { path };
    let query = m.get(3).map_or("", |q| q.as_str());
    let internal = re(
        &INTERNAL,
        r"(?i)^(?:localhost|127\.[0-9]+\.[0-9]+\.[0-9]+|0\.0\.0\.0|10\.[0-9]+\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+|172\.(?:1[6-9]|2[0-9]|3[01])\.[0-9]+\.[0-9]+|\[::1\]|[^.]+|.+\.(?:local|internal|intranet|corp|lan|home|localhost))$",
    );
    if internal.is_match(&host) {
        return true;
    }
    if share_link(&host, path) {
        return true;
    }
    let secret = re(
        &SECRET,
        r"(?i)^(?:token|access_token|id_token|refresh_token|auth|authuser|key|api_?key|apikey|secret|sig|signature|password|pass|pwd|session|sessionid|sid|code|otp|invite|share|sharing|usp|x-amz-[a-z-]+|id|uid|userid|docid|fileid|[a-z]+_id)$",
    );
    let random = re(&RANDOM, r"^[A-Za-z0-9_\-.%~+/=]+$");
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.split('=');
        let k = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        if secret.is_match(k) && !v.is_empty() {
            return true;
        }
        let looks_random = v.encode_utf16().count() >= 20
            && v.chars().any(|c| c.is_ascii_alphabetic())
            && v.chars().any(|c| c.is_ascii_digit())
            && random.is_match(v);
        if looks_random {
            return true;
        }
    }
    false
}

/// A host test and the pattern for the start of the path.
type ShareRule = (fn(&str) -> bool, &'static str);

/// A share link to a private document, meeting or file: its host (lower case), then the start of its path.
fn share_link(host: &str, path: &str) -> bool {
    static PATHS: OnceLock<Vec<Regex>> = OnceLock::new();
    let rules: [ShareRule; 13] = [
        (|h| h == "docs.google.com", r"^/(?:document|spreadsheets|presentation|forms|drawings)/(?:u/[0-9]+/)?d/"),
        (|h| h == "drive.google.com", r"^/(?:file/d/|drive/folders/|drive/u/|open)"),
        (|h| h == "meet.google.com", r"^/[a-z]"),
        (|h| h == "zoom.us" || h.ends_with(".zoom.us"), r"^/(?:j|my|s|rec)/"),
        (|h| h == "teams.microsoft.com", r"^/l/"),
        (|h| h == "dropbox.com" || h.ends_with(".dropbox.com"), r"^/(?:s|sh|scl)/"),
        (|h| h == "1drv.ms", r"^/"),
        (|h| h == "onedrive.live.com", r"^/"),
        (|h| h.ends_with(".sharepoint.com"), r"^/"),
        (|h| h == "figma.com" || h.ends_with(".figma.com"), r"^/(?:file|design|proto|board|deck)/"),
        (|h| h == "notion.so" || h.ends_with(".notion.so"), r"^/"),
        (|h| h == "app.box.com", r"^/s/"),
        (|h| h == "wetransfer.com" || h.ends_with(".wetransfer.com"), r"^/downloads/"),
    ];
    let paths = PATHS.get_or_init(|| rules.iter().map(|(_, p)| Regex::new(p).expect("share link path")).collect());
    rules.iter().zip(paths).any(|((host_ok, _), p)| host_ok(host) && p.is_match(path))
}

// ---------- names in "People, links and files" ----------

/// Words that start a People bullet but aren't a person (things people list there: places, sites, tools).
const NOT_NAMES: &[&str] = &[
    "blog", "post", "posts", "site", "website", "page", "link", "links", "repo", "repos", "repository", "docs", "doc",
    "document", "documents", "file", "files", "folder", "profile", "account", "email", "emails", "phone", "github",
    "gitlab", "bitbucket", "slack", "figma", "notion", "google", "drive", "dropbox", "jira", "linear", "trello", "asana",
    "discord", "twitter", "linkedin", "youtube", "instagram", "facebook", "medium", "hashnode", "substack", "wikipedia",
    "reddit", "zoom", "teams", "meet", "calendar", "spreadsheet", "sheet", "sheets", "slides", "deck", "notes", "note",
    "project", "projects", "team", "client", "clients", "company", "app", "apps", "tool", "tools", "dataset",
    "datasets", "paper", "papers", "article", "articles", "course", "video", "videos", "me", "myself", "you", "they",
    "we", "us", "them", "none", "other", "others",
];
/// Small words that can sit inside a name ("Ana de la Cruz", "Ludwig van Beethoven").
const PARTICLES: &[&str] = &[
    "de", "la", "le", "da", "das", "do", "dos", "del", "della", "di", "du", "van", "von", "der", "den", "ter", "bin",
    "binti", "al", "el", "y", "e",
];

/// One name word: a capital then lower-case letters ("Ana", "O'Neil", "Ruiz-Lopez", "McKay"); not "GitHub", "API".
fn name_word(w: &str) -> bool {
    static NAME: OnceLock<Regex> = OnceLock::new();
    re(&NAME, r"^(?:\p{Lu}['’])?(?:Mc|Mac)?\p{Lu}\p{Ll}+(?:[-'’]\p{Lu}?\p{Ll}+)*\.?$").is_match(w)
}

/// `trim()` as JS does it.
fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// The name part of each bullet in the `## People…` section ("- Ana Ruiz — client", "- **Dev Raj** (engineer)"):
/// 1–4 name words, on a line with no link or path, that isn't a thing like "Blog" or "GitHub profile".
pub fn bullet_names(text: &str) -> Vec<String> {
    static LINK_OR_PATH: OnceLock<Regex> = OnceLock::new();
    let link_or_path = re(&LINK_OR_PATH, r"https?://|www\.|\S/\S|~/|[A-Za-z]:\\");
    let mut names = vec![];
    let mut in_people = false;
    for raw in text.split('\n') {
        let line = js_trim(raw);
        if line.starts_with('#') {
            in_people = line.starts_with("## ") && js_trim(&line[3..]).to_lowercase().starts_with("people");
            continue;
        }
        if !in_people || !(line.starts_with("- ") || line.starts_with("* ")) {
            continue;
        }
        if link_or_path.is_match(line) {
            continue; // a link or a file, not a person
        }
        let mut item = line[2..].replace("**", "").replace("__", "");
        for stop in [" — ", " – ", " - ", ":", "(", ",", ";", "<", " <"] {
            if let Some(at) = item.find(stop) {
                item.truncate(at);
            }
        }
        let item = js_trim(&item);
        let words: Vec<&str> = item.split(is_space).filter(|w| !w.is_empty()).collect();
        let (Some(first), Some(last)) = (words.first(), words.last()) else { continue };
        if words.len() > 4 {
            continue;
        }
        let ok = name_word(first)
            && name_word(last)
            && words.iter().all(|w| name_word(w) || PARTICLES.contains(w))
            && !words.iter().any(|w| {
                let lower = w.to_lowercase();
                NOT_NAMES.contains(&lower.strip_suffix('.').unwrap_or(&lower))
            });
        if ok {
            names.push(item.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/share-check.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).expect("share-check.json")
    }

    fn strings(v: &Value) -> Vec<String> {
        v.as_array().expect("a list").iter().map(|s| s.as_str().expect("a string").to_string()).collect()
    }

    #[test]
    fn shared_find_cases() {
        let all = cases();
        let finds = all["find"].as_array().expect("find cases");
        assert!(!finds.is_empty());
        for case in finds {
            let name = case["name"].as_str().unwrap_or("?");
            // The shared window checks eight kinds; this window shows the first four.
            assert!(case["expect"].as_object().is_some(), "{name}: expect");
            let found = find(case["text"].as_str().expect("text"));
            for kind in Kind::ALL {
                assert_eq!(found.get(kind), strings(&case["expect"][kind.key()]).as_slice(), "{name}: {}", kind.key());
            }
        }
    }

    #[test]
    fn shared_remove_cases() {
        let all = cases();
        for case in all["remove"].as_array().expect("remove cases") {
            let got = remove(case["text"].as_str().unwrap(), &strings(&case["remove"]));
            assert_eq!(got, case["expect"].as_str().unwrap());
        }
    }

    #[test]
    fn front_matter_too_and_short_items_never_match_inside_removed() {
        let text = "---\ntitle: Re: Ana\n---\n\nAna said re.\n";
        assert_eq!(remove(text, &["re".into(), "Ana".into()]), "---\ntitle: Re: [removed]\n---\n\n[removed] said [removed].\n");
        assert_eq!(remove("removed re", &["re".into(), "removed".into()]), "[removed] [removed]");
        assert_eq!(body_start("---\na: b\n---\nbody"), "---\na: b\n---".len());
        assert_eq!(body_start("---\r\na: b\r\n---\r\nbody"), "---\r\na: b\r\n---\r".len());
        assert_eq!(body_start("---\na: b\n--- \n"), "---\na: b\n--- ".len());
        assert_eq!(body_start("---\na: b\n----\nbody"), 0, "no closing line");
        assert_eq!(body_start("no front matter"), 0);
    }

    #[test]
    fn groups_and_items() {
        let f = find("Write to a@b.co or call +1 415 555 0132; see https://x.io/a and http://localhost:8080/x.");
        assert_eq!(f.emails, ["a@b.co"]);
        assert_eq!(f.phones, ["+1 415 555 0132"]);
        assert_eq!(f.links, ["http://localhost:8080/x"], "the public link is not listed");
        let kinds: Vec<Kind> = f.groups().iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, [Kind::Emails, Kind::Phones, Kind::Links]);
        assert_eq!(f.items().len(), 3);
        assert!(!f.is_empty() && find("Nothing here.").is_empty());
        assert_eq!(Kind::Phones.title(), "Phone numbers");
        assert_eq!(Kind::Links.title(), "Private links");
        assert!(find("Paid $12,000 to @devraj for ~/Documents/a.xlsx and /usr/local/bin.").is_empty(), "no money, handles or files");
    }

    #[test]
    fn people_section() {
        let text = "## People, links and files\n- GitHub\n- Blog: https://saachis-blog.hashnode.dev/\n- Post\n\
                    - **Priya Shah** — head of sales\n- Ana de la Cruz; designer\n- Seán O'Neil <sean@x.io>\n- McKay\n\
                    - API docs\n- Dev Raj (engineer)\n- de la Cruz\n- One Two Three Four Five\n\n### Notes\n- Not Here\n";
        assert_eq!(bullet_names(text), ["Priya Shah", "Ana de la Cruz", "Seán O'Neil", "McKay", "Dev Raj"]);
        let f = find(text);
        assert_eq!(f.people, ["Priya Shah", "Ana de la Cruz", "Seán O'Neil", "McKay", "Dev Raj"]);
        assert_eq!(f.emails, ["sean@x.io"]);
        assert!(f.links.is_empty(), "a public blog is not private");
        assert!(bullet_names("## In short\n- Ana Ruiz\n").is_empty(), "only in the People section");
    }

    #[test]
    fn private_links() {
        for url in [
            "http://localhost:5173/admin",
            "https://user:pw@example.com/",
            "http://intranet/home",
            "https://docs.google.com/spreadsheets/u/0/d/abc/edit",
            "https://meet.google.com/abc-defg-hij",
            "https://x.com/?q=a1b2c3d4e5f6g7h8i9j0k",
            "https://shop.com/order?Order_ID=8",
        ] {
            assert!(is_private_link(url), "{url}");
        }
        for url in [
            "https://saachis-blog.hashnode.dev/overfitting",
            "https://github.com/Saachi26/ML-Journey",
            "https://acme.com/pricing?page=2",
            "https://www.figma.com/community",
            "https://x.com/?token=",
        ] {
            assert!(!is_private_link(url), "{url}");
        }
    }

    #[test]
    fn phones_and_emails_edges() {
        let f = find("Call (020) 7946 0958, 415-555-0132abc or 2026-10-19 10:14. Order 1234. Q4 12345678x.");
        assert_eq!(f.phones, ["(020) 7946 0958"]);
        let f = find("Mail a@b@c.com, x@y.co., .me@x.org and @me.org.");
        assert_eq!(f.emails, ["b@c.com", "x@y.co", ".me@x.org"]);
    }
}
