// context-capsule — core: "Check before sharing" — what in a capsule might be private (people, emails, phone
// numbers, links, money, files), found with plain rules (no AI), and a copy with the checked items replaced by
// `[removed]`. The same rules as the Mac (SPEC.md); spec/share-check.json holds the cases both must pass.
//! Only the body is read and changed: a leading `---` front matter block is skipped and kept as it is.

/// One group of things that might be private.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    People,
    Emails,
    Phones,
    Links,
    Money,
    Files,
}

impl Kind {
    /// In the order they are shown.
    pub const ALL: [Kind; 6] = [Kind::People, Kind::Emails, Kind::Phones, Kind::Links, Kind::Money, Kind::Files];

    /// The group's heading.
    pub fn title(self) -> &'static str {
        match self {
            Kind::People => "People",
            Kind::Emails => "Emails",
            Kind::Phones => "Phone numbers",
            Kind::Links => "Links",
            Kind::Money => "Money",
            Kind::Files => "Files",
        }
    }

    /// Its key in spec/share-check.json.
    pub fn key(self) -> &'static str {
        match self {
            Kind::People => "people",
            Kind::Emails => "emails",
            Kind::Phones => "phones",
            Kind::Links => "links",
            Kind::Money => "money",
            Kind::Files => "files",
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
    pub money: Vec<String>,
    pub files: Vec<String>,
}

impl Found {
    pub fn get(&self, kind: Kind) -> &[String] {
        match kind {
            Kind::People => &self.people,
            Kind::Emails => &self.emails,
            Kind::Phones => &self.phones,
            Kind::Links => &self.links,
            Kind::Money => &self.money,
            Kind::Files => &self.files,
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

/// Where the body starts (byte offset): after a leading `---` front matter block, else 0.
pub fn body_start(text: &str) -> usize {
    let Some(rest) = text.strip_prefix("---\n") else { return 0 };
    let Some(end) = rest.find("\n---") else { return 0 };
    let after = 4 + end + 4; // "---\n" + front + "\n---"
    // The rest of the closing line ("---" and its newline).
    match text[after..].find('\n') {
        Some(nl) => after + nl + 1,
        None => text.len(),
    }
}

/// Find what might be private in a capsule's body.
pub fn find(text: &str) -> Found {
    let body = &text[body_start(text)..];
    let mut work: Vec<char> = body.chars().collect();

    // 1. Links, then emails; each blanked out (same length) so later rules can't match inside them.
    let links = find_links(&work);
    blank(&mut work, &links);
    let emails = find_emails(&work);
    blank(&mut work, &emails);
    // 2–5.
    let files = find_files(&work);
    let phones = find_phones(&work);
    let money = find_money(&work);
    let mut people = find_handles(&work);
    people.extend(people_section(body));

    let tidy = |spans: Vec<String>| ordered(body, spans);
    let texts = |spans: &[(usize, usize)]| spans.iter().map(|(a, b)| work_text(body, *a, *b)).collect::<Vec<_>>();
    Found {
        people: tidy(people),
        emails: tidy(texts(&emails)),
        phones: tidy(phones),
        links: tidy(texts(&links)),
        money: tidy(money),
        files: tidy(files),
    }
}

/// A copy of `text` with every occurrence of each of `items` (longest first) in the body replaced by `[removed]`.
/// The front matter stays as it is.
pub fn remove(text: &str, items: &[String]) -> String {
    // The whole file, front matter too: a name in the title is just as private.
    let mut items: Vec<&str> = items.iter().map(String::as_str).filter(|i| !i.is_empty()).collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.len()));
    items.dedup();
    let body = text;
    let mut out = String::with_capacity(text.len());
    // One pass, left to right, longest item first at each place: a short item never matches inside a
    // `[removed]` put there for a longer one.
    let mut i = 0;
    'scan: while i < body.len() {
        for item in &items {
            if body[i..].starts_with(item) {
                out.push_str(REMOVED);
                i += item.len();
                continue 'scan;
            }
        }
        let c = body[i..].chars().next().expect("inside the body");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// The body's text for chars `a..b` (the working copy only differs where things were blanked).
fn work_text(body: &str, a: usize, b: usize) -> String {
    body.chars().skip(a).take(b - a).collect()
}

/// Each item once, in order of first appearance in the body.
fn ordered(body: &str, items: Vec<String>) -> Vec<String> {
    let mut seen: Vec<(usize, String)> = vec![];
    for item in items {
        if item.is_empty() || seen.iter().any(|(_, s)| *s == item) {
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

/// Drop trailing `. , ; :` from chars `a..b`.
fn trim_end(work: &[char], a: usize, mut b: usize) -> usize {
    while b > a && matches!(work[b - 1], '.' | ',' | ';' | ':') {
        b -= 1;
    }
    b
}

fn text(work: &[char], a: usize, b: usize) -> String {
    work[a..b].iter().collect()
}

/// `https?://` up to whitespace, `)`, `>` or `]`; trailing `. , ; :` dropped.
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
        if scheme == 0 {
            i += 1;
            continue;
        }
        let mut j = i + scheme;
        while j < work.len() && !work[j].is_whitespace() && !matches!(work[j], ')' | '>' | ']') {
            j += 1;
        }
        let end = trim_end(work, i, j);
        if end > i + scheme {
            out.push((i, end));
        }
        i = j.max(i + 1);
    }
    out
}

fn is_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

fn is_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-')
}

/// `name@domain.tld`.
fn find_emails(work: &[char]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = vec![];
    for at in 0..work.len() {
        if work[at] != '@' || out.last().is_some_and(|(_, b)| at < *b) {
            continue;
        }
        let mut a = at;
        while a > 0 && is_local(work[a - 1]) {
            a -= 1;
        }
        while a < at && work[a] == '.' {
            a += 1;
        }
        let mut b = at + 1;
        while b < work.len() && is_domain(work[b]) {
            b += 1;
        }
        while b > at + 1 && matches!(work[b - 1], '.' | '-') {
            b -= 1;
        }
        let domain = text(work, at + 1, b);
        let tld_ok = domain.rsplit_once('.').is_some_and(|(name, tld)| {
            !name.is_empty() && tld.chars().count() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
        });
        if a < at && tld_ok {
            out.push((a, b));
        }
    }
    out
}

/// `~/…` or `/…` with at least two parts, or `C:\…`, starting after start-of-line, whitespace or `(`.
fn find_files(work: &[char]) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < work.len() {
        let after_ok = i == 0 || work[i - 1].is_whitespace() || work[i - 1] == '(';
        let unix = work[i] == '/' || starts_with(work, i, "~/");
        let windows = work[i].is_ascii_alphabetic() && work.get(i + 1) == Some(&':') && work.get(i + 2) == Some(&'\\');
        if !after_ok || !(unix || windows) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < work.len() && !work[j].is_whitespace() && !matches!(work[j], ')' | ']' | '>' | '"' | '`') {
            j += 1;
        }
        let end = trim_end(work, i, j);
        let path = text(work, i, end);
        let ok = if windows {
            path.chars().count() > 3
        } else {
            let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix('/')).unwrap_or("");
            !rest.starts_with('/') && rest.split('/').filter(|p| !p.is_empty()).count() >= 2
        };
        if ok {
            out.push(path);
        }
        i = j.max(i + 1);
    }
    out
}

fn is_phone_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '(' | ')')
}

/// `dddd-dd-dd` somewhere in it: a date, not a phone.
fn has_date(s: &[char]) -> bool {
    let shape = "dddd-dd-dd";
    s.windows(shape.len()).any(|w| {
        w.iter().zip(shape.chars()).all(|(c, want)| if want == 'd' { c.is_ascii_digit() } else { *c == want })
    })
}

/// `+`/digit, then digits, spaces, `-`, `.`, `(`, `)`, ending in a digit, with 8–15 digits; not dates.
fn find_phones(work: &[char]) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < work.len() {
        let c = work[i];
        let starts = (c == '+' || c.is_ascii_digit()) && (i == 0 || !work[i - 1].is_alphanumeric());
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < work.len() && is_phone_char(work[j]) {
            j += 1;
        }
        let mut end = j;
        while end > i && !work[end - 1].is_ascii_digit() {
            end -= 1;
        }
        let digits = work[i..end].iter().filter(|c| c.is_ascii_digit()).count();
        if (8..=15).contains(&digits) && !has_date(&work[i..end]) {
            out.push(text(work, i, end));
        }
        i = j;
    }
    out
}

const CURRENCY: [char; 4] = ['$', '€', '£', '₹'];
const CODES: [&str; 4] = ["USD", "EUR", "GBP", "INR"];
const WORDS: [&str; 4] = ["dollars", "euros", "pounds", "rupees"];

/// Digits with `,` and `.` from `i`; its end (trailing `,` `.` dropped). None: no digit there.
fn amount(work: &[char], i: usize) -> Option<usize> {
    if !work.get(i)?.is_ascii_digit() {
        return None;
    }
    let mut j = i;
    while j < work.len() && (work[j].is_ascii_digit() || matches!(work[j], ',' | '.')) {
        j += 1;
    }
    while j > i && matches!(work[j - 1], ',' | '.') {
        j -= 1;
    }
    Some(j)
}

fn letter_at(work: &[char], i: usize) -> bool {
    work.get(i).is_some_and(|c| c.is_alphabetic())
}

/// A currency sign and an amount (`$12,000`, `€450`, `$1.5m`), or an amount and a code or word (`1,200 USD`).
fn find_money(work: &[char]) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < work.len() {
        if CURRENCY.contains(&work[i]) {
            if let Some(mut end) = amount(work, i + 1) {
                // A short scale: k, m, b, bn.
                for suffix in ["bn", "k", "m", "b", "K", "M", "B"] {
                    let n = suffix.chars().count();
                    if starts_with(work, end, suffix) && !letter_at(work, end + n) {
                        end += n;
                        break;
                    }
                }
                out.push(text(work, i, end));
                i = end;
                continue;
            }
        }
        let boundary = i == 0 || !(work[i - 1].is_alphanumeric() || CURRENCY.contains(&work[i - 1]) || matches!(work[i - 1], ',' | '.'));
        if boundary {
            if let Some(end) = amount(work, i) {
                if work.get(end) == Some(&' ') {
                    let word_at = end + 1;
                    let mut w_end = word_at;
                    while w_end < work.len() && work[w_end].is_alphabetic() {
                        w_end += 1;
                    }
                    let word = text(work, word_at, w_end);
                    if CODES.contains(&word.as_str()) || WORDS.contains(&word.to_lowercase().as_str()) {
                        out.push(text(work, i, w_end));
                        i = w_end;
                        continue;
                    }
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_handle(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// `@handles` (emails are blanked out by now).
fn find_handles(work: &[char]) -> Vec<String> {
    let mut out = vec![];
    for i in 0..work.len() {
        if work[i] != '@' || (i > 0 && (work[i - 1].is_alphanumeric() || work[i - 1] == '@')) {
            continue;
        }
        let mut j = i + 1;
        while j < work.len() && is_handle(work[j]) {
            j += 1;
        }
        while j > i + 1 && matches!(work[j - 1], '.' | '-') {
            j -= 1;
        }
        if j > i + 1 && work[i + 1..j].iter().any(|c| c.is_ascii_alphanumeric()) {
            out.push(text(work, i, j));
        }
    }
    out
}

/// In the `## People…` section: the start of each `-`/`*` bullet (bold removed) up to ` — `, ` – `, ` - `,
/// `:`, `(` or `,`, when it is 1–4 words that each start with a capital letter, with no digits, `/`, `@` or `http`.
fn people_section(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut inside = false;
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with("## ") || t.starts_with("# ") {
            inside = t.starts_with("## ") && t[3..].trim_start().to_lowercase().starts_with("people");
            continue;
        }
        if !inside {
            continue;
        }
        let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else { continue };
        let item = item.replace("**", "");
        let cut = [" — ", " – ", " - ", ":", "(", ","].iter().filter_map(|s| item.find(s)).min().unwrap_or(item.len());
        let name = item[..cut].trim();
        let words: Vec<&str> = name.split_whitespace().collect();
        let looks_like_name = (1..=4).contains(&words.len())
            && words.iter().all(|w| w.chars().next().is_some_and(char::is_uppercase))
            && !name.chars().any(|c| c.is_ascii_digit() || c == '/' || c == '@')
            && !name.to_lowercase().contains("http");
        if looks_like_name {
            out.push(words.join(" "));
        }
    }
    out
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
        assert_eq!(body_start("---\na: b\n---\nbody"), "---\na: b\n---\n".len());
        assert_eq!(body_start("no front matter"), 0);
    }

    #[test]
    fn groups_and_items() {
        let f = find("Write to a@b.co about $5k and see https://x.io/a.");
        assert_eq!(f.emails, ["a@b.co"]);
        assert_eq!(f.money, ["$5k"]);
        assert_eq!(f.links, ["https://x.io/a"]);
        let kinds: Vec<Kind> = f.groups().iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, [Kind::Emails, Kind::Links, Kind::Money]);
        assert_eq!(f.items().len(), 3);
        assert!(!f.is_empty() && find("Nothing here.").is_empty());
        assert_eq!(Kind::Phones.title(), "Phone numbers");
    }

    #[test]
    fn edges() {
        let f = find("Costs $1.5m, or 300 euros. Path /etc is short; /usr/local/bin is not. Call (020) 7946 0958 or 2026-10-19.");
        assert_eq!(f.money, ["$1.5m", "300 euros"]);
        assert_eq!(f.files, ["/usr/local/bin"]);
        assert_eq!(f.phones, ["020) 7946 0958"]);
        let f = find("Mail me@x.org, not @me.org. Ping @ana_b.");
        assert_eq!(f.emails, ["me@x.org"]);
        assert_eq!(f.people, ["@me.org", "@ana_b"]);
    }
}
