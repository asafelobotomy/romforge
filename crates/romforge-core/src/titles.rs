//! How alike two game titles are, for suggestions (milestone 12.5).
//!
//! Titles are compared by their words after normalizing (tags dropped,
//! articles, accents, "&", punctuation, Roman numerals), with the subtitle
//! (after " - " or ":") as a field of its own. Hard tokens (numbers, years,
//! and edition words such as "Jr." or "Turbo") must agree, or the score is
//! 0: "Street Fighter II" never suggests "Street Fighter III". A score is
//! never a match (house rule 8); it only orders suggestions.

use std::collections::{BTreeSet, HashMap};

#[cfg(test)]
mod tests;

/// Words that name a different game or edition when they differ.
const EDITION_WORDS: &[&str] = &[
    "jr",
    "deluxe",
    "dx",
    "plus",
    "turbo",
    "special",
    "ex",
    "hd",
    "remix",
    "championship",
    "gold",
    "silver",
    "crystal",
    "red",
    "blue",
    "green",
    "yellow",
];

/// Articles dropped wherever they are ("Legend of Zelda, The" and "The
/// Legend of Zelda" are one title).
const ARTICLES: &[&str] = &["the", "a", "an"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleKey {
    /// Words of the main title.
    pub main: Vec<String>,
    /// Words of the subtitle (after " - " or ":"), if any.
    pub sub: Vec<String>,
    /// Numbers and edition words, from both parts.
    pub hard: BTreeSet<String>,
}

/// Roman numerals II to XX as digits (not a lone V or X). A lone "I" counts only at the end
/// of a main title ("Final Fantasy I"), since it is also a word.
fn roman(w: &str, last: bool) -> Option<&'static str> {
    const R: &[(&str, &str)] = &[
        ("ii", "2"),
        ("iii", "3"),
        ("iv", "4"),
        ("vi", "6"),
        ("vii", "7"),
        ("viii", "8"),
        ("ix", "9"),
        ("xi", "11"),
        ("xii", "12"),
        ("xiii", "13"),
        ("xiv", "14"),
        ("xv", "15"),
        ("xvi", "16"),
        ("xvii", "17"),
        ("xviii", "18"),
        ("xix", "19"),
        ("xx", "20"),
    ];
    // A lone V or X is too often a name ("Mega Man X", "X-Men", "V-Rally")
    // to be a number; DAT names keep their numerals as written anyway.
    if w == "i" {
        return last.then_some("1");
    }
    R.iter().find(|(r, _)| *r == w).map(|(_, d)| *d)
}

fn words(part: &str) -> Vec<String> {
    let folded = crate::extdb::fold_accents(part).replace('&', " and ");
    let raw: Vec<String> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .filter(|w| !ARTICLES.contains(&w.as_str()))
        .collect();
    let n = raw.len();
    raw.iter()
        .enumerate()
        .map(|(i, w)| roman(w, i + 1 == n).map_or_else(|| w.clone(), str::to_owned))
        .collect()
}

/// The comparable form of a title (a DAT name, a file name, a label).
pub fn key(title: &str) -> TitleKey {
    // Tags, the extension and an archive member's path go first.
    let t = title.rsplit(['/', '#']).next().unwrap_or(title);
    let t = crate::view::search_title(&crate::names::parse(t).title);
    let (main, sub) = match t.split_once(" - ").or_else(|| t.split_once(": ")) {
        Some((m, s)) => (m, s),
        None => (t.as_str(), ""),
    };
    let mut main = words(main);
    // A first game is often named without its "1" ("Shin Megami Tensei 1"
    // is the DAT's "Shin Megami Tensei").
    if main.len() > 1 && main.last().is_some_and(|w| w == "1") {
        main.pop();
    }
    let sub = words(sub);
    let hard = main.iter().chain(&sub).filter(|w| is_hard(w)).cloned().collect();
    TitleKey { main, sub, hard }
}

/// Two words alike: equal, or (both long enough) one small typo apart.
fn alike(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 5 && b.len() >= 5 && strsim::normalized_levenshtein(a, b) >= 0.8)
}

/// The words of `a` found in `b`, each word of `b` used once.
fn shared<'a>(a: &'a [String], b: &[String]) -> Vec<&'a String> {
    let mut left: Vec<&String> = b.iter().collect();
    a.iter()
        .filter(|w| match left.iter().position(|x| alike(w, x)) {
            Some(i) => {
                left.swap_remove(i);
                true
            }
            None => false,
        })
        .collect()
}

/// Shared words over all words (Dice).
fn dice(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    2.0 * shared(a, b).len() as f32 / (a.len() + b.len()) as f32
}

fn is_hard(w: &str) -> bool {
    w.bytes().all(|b| b.is_ascii_digit()) || EDITION_WORDS.contains(&w)
}

/// How alike two keys are, 0 to 1:
/// - 1.0: the same words;
/// - 0.9: the same main title, one side has no subtitle;
/// - 0.85: every word of one side (two at least) is in the other
///   ("zelda link to the past");
/// - otherwise word overlap (main title weighs double);
/// - 0 when their numbers or edition words differ, or they share no other
///   word ("Alien 3" and "Dragon Quest III" only share the 3).
pub fn score_keys(a: &TitleKey, b: &TitleKey) -> f32 {
    if a.main.is_empty() || b.main.is_empty() || a.hard != b.hard {
        return 0.0;
    }
    // The same words: the same title, even one made of numbers ("1942").
    if a.main == b.main && a.sub == b.sub {
        return 1.0;
    }
    let all_a: Vec<String> = a.main.iter().chain(&a.sub).cloned().collect();
    let all_b: Vec<String> = b.main.iter().chain(&b.sub).cloned().collect();
    let common = shared(&all_a, &all_b);
    if !common.iter().any(|w| !is_hard(w)) {
        return 0.0;
    }
    let main = dice(&a.main, &b.main);
    if main == 1.0 && a.main == b.main {
        if a.sub == b.sub {
            return 1.0;
        }
        if a.sub.is_empty() || b.sub.is_empty() {
            return 0.9;
        }
    }
    let fewer = all_a.len().min(all_b.len());
    if fewer >= 2 && common.len() == fewer {
        return 0.85;
    }
    // Never up to the "every word" band.
    ((2.0 * main + dice(&all_a, &all_b)) / 3.0).min(0.84)
}

/// The words of a text (a readme) as titles are compared.
pub fn text_words(text: &str) -> Vec<String> {
    words(text)
}

/// Whether a text names a title: the title's words (two at least, or one
/// long one) in a row in it, not followed by a number the title doesn't
/// have ("Phantasy Star 2" doesn't name "Phantasy Star"). How many words
/// matched, or 0.
pub fn named_in(title: &TitleKey, text: &[String]) -> usize {
    let want: Vec<&String> = title.main.iter().chain(&title.sub).collect();
    let main = title.main.len();
    if main == 0 || (main == 1 && title.main[0].len() < 6 && title.sub.is_empty()) {
        return 0;
    }
    let runs = |n: usize| {
        let w = &want[..n];
        text.windows(n).enumerate().any(|(i, win)| {
            win.iter().zip(w).all(|(a, b)| a == *b)
                && text
                    .get(i + n)
                    .is_none_or(|next| !is_hard(next) || title.hard.contains(next))
        })
    };
    if runs(want.len()) {
        want.len()
    } else if want.len() > main && runs(main) {
        main
    } else {
        0
    }
}

/// [`named_in`] for many titles at once (every DAT game): the titles a
/// text names, by looking up each run of its words.
#[derive(Default)]
pub struct NameIndex {
    /// (position in what [`NameIndex::new`] was given, key)
    keys: Vec<(usize, TitleKey)>,
    /// The words of a title (all of them, or its main title) joined by
    /// spaces -> the titles.
    runs: HashMap<String, Vec<usize>>,
}

/// The longest title run [`NameIndex`] looks up.
const MAX_RUN: usize = 12;

impl NameIndex {
    pub fn new<'a>(titles: impl IntoIterator<Item = &'a str>) -> Self {
        let mut keys = Vec::new();
        let mut runs: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, t) in titles.into_iter().enumerate() {
            let k = key(t);
            let main = k.main.len();
            if main == 0 || (main == 1 && k.main[0].len() < 6 && k.sub.is_empty()) {
                continue;
            }
            let i = keys.len();
            let all: Vec<&str> = k.main.iter().chain(&k.sub).map(String::as_str).collect();
            if all.len() <= MAX_RUN {
                runs.entry(all.join(" ")).or_default().push(i);
            }
            if !k.sub.is_empty() && main <= MAX_RUN {
                runs.entry(k.main.join(" ")).or_default().push(i);
            }
            keys.push((at, k));
        }
        NameIndex { keys, runs }
    }

    /// The titles (their positions in what [`NameIndex::new`] was given) a
    /// text names with the most words.
    pub fn named(&self, text: &[String]) -> Vec<usize> {
        let mut best = (0usize, BTreeSet::new());
        for i in 0..text.len() {
            for n in 1..=MAX_RUN.min(text.len() - i) {
                let Some(hits) = self.runs.get(&text[i..i + n].join(" ")) else {
                    continue;
                };
                for &h in hits {
                    let k = &self.keys[h].1;
                    let fits = text
                        .get(i + n)
                        .is_none_or(|next| !is_hard(next) || k.hard.contains(next));
                    if !fits || n < best.0 {
                        continue;
                    }
                    if n > best.0 {
                        best = (n, BTreeSet::new());
                    }
                    best.1.insert(h);
                }
            }
        }
        best.1.into_iter().map(|h| self.keys[h].0).collect()
    }
}

/// [`score_keys`] for two titles.
pub fn score(a: &str, b: &str) -> f32 {
    score_keys(&key(a), &key(b))
}

/// Whether a cartridge header's title fits a DAT title. Headers squash
/// spaces and cut titles short ("DRAGONQUEST3", "SUPER MARIOWORLD", 21
/// characters at most), so the words are compared run together, the
/// header's as a start of the DAT title's (six letters at least).
pub fn header_fits(header: &str, title: &str) -> bool {
    let run = |k: &TitleKey| {
        k.main
            .iter()
            .chain(&k.sub)
            .map(String::as_str)
            .collect::<String>()
    };
    let (hk, tk) = (key(header), key(title));
    let (h, t) = (run(&hk), run(&tk));
    // A cut-off header is the start of the title, but its numbers still
    // count: "MEGAMAN2" is not Mega Man 20, nor "ZELDA 2" plain Zelda.
    let cut_number = h.ends_with(|c: char| c.is_ascii_digit())
        && t[h.len().min(t.len())..].starts_with(|c: char| c.is_ascii_digit());
    score(header, title) >= 0.85
        || (h.len() >= 6 && t.starts_with(&h) && !cut_number && hk.hard.is_subset(&tk.hard))
}

/// DAT titles, keyed once, found by any shared word. Grouped by their hard
/// tokens: a title whose numbers or edition words differ from the query's
/// scores 0 anyway, so only the query's own group is looked at (a common
/// word like "game" or "super" no longer makes every title a candidate).
#[derive(Debug, Default)]
pub struct TitleIndex {
    entries: Vec<(TitleKey, String)>,
    /// hard tokens -> word -> entries
    groups: HashMap<BTreeSet<String>, HashMap<String, Vec<usize>>>,
}

impl TitleIndex {
    pub fn new<I: IntoIterator<Item = String>>(titles: I) -> Self {
        let mut idx = TitleIndex::default();
        let mut seen = BTreeSet::new();
        for t in titles {
            if !seen.insert(t.clone()) {
                continue;
            }
            let k = key(&t);
            let n = idx.entries.len();
            let group = idx.groups.entry(k.hard.clone()).or_default();
            for w in k.main.iter().chain(&k.sub).collect::<BTreeSet<_>>() {
                group.entry(w.clone()).or_default().push(n);
            }
            idx.entries.push((k, t));
        }
        idx
    }

    /// Titles alike to `text`, best first, ties by name; at most `limit`.
    pub fn find(&self, text: &str, limit: usize) -> Vec<(String, f32)> {
        let q = key(text);
        let Some(group) = self.groups.get(&q.hard) else {
            return Vec::new();
        };
        let mut ids: Vec<usize> = q
            .main
            .iter()
            .chain(&q.sub)
            .filter_map(|w| group.get(w))
            .flatten()
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut out: Vec<(String, f32)> = ids
            .into_iter()
            .filter_map(|i| {
                let (k, t) = &self.entries[i];
                let s = score_keys(&q, k);
                (s > 0.0).then(|| (t.clone(), s))
            })
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out.truncate(limit);
        out
    }
}
