//! How alike two game titles are, for suggestions (milestone 12.5).
//!
//! Titles are compared by their words after normalizing (tags dropped,
//! articles, accents, "&", punctuation, Roman numerals), with the subtitle
//! (after " - " or ":") as a field of its own. Hard tokens (numbers, years,
//! and edition words such as "Jr." or "Turbo") must agree, or the score is
//! 0: "Street Fighter II" never suggests "Street Fighter III". A score is
//! never a match (house rule 8); it only orders suggestions.

use std::collections::{BTreeSet, HashMap};

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
    let main = words(main);
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
    let (h, t) = (run(&key(header)), run(&key(title)));
    score(header, title) >= 0.85 || (h.len() >= 6 && t.starts_with(&h))
}

/// DAT titles, keyed once, found by any shared word.
#[derive(Debug, Default)]
pub struct TitleIndex {
    entries: Vec<(TitleKey, String)>,
    by_word: HashMap<String, Vec<usize>>,
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
            for w in k.main.iter().chain(&k.sub).collect::<BTreeSet<_>>() {
                idx.by_word.entry(w.clone()).or_default().push(n);
            }
            idx.entries.push((k, t));
        }
        idx
    }

    /// Titles alike to `text`, best first, ties by name; at most `limit`.
    pub fn find(&self, text: &str, limit: usize) -> Vec<(String, f32)> {
        let q = key(text);
        let mut ids: Vec<usize> = q
            .main
            .iter()
            .chain(&q.sub)
            .filter_map(|w| self.by_word.get(w))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_normalize() {
        let k = key("Legend of Zelda, The - A Link to the Past (USA)");
        assert_eq!(k.main, ["legend", "of", "zelda"]);
        assert_eq!(k.sub, ["link", "to", "past"]);
        assert_eq!(key("The Legend of Zelda: A Link to the Past").main, k.main);
        assert_eq!(
            key("Final Fantasy VII (USA) (Disc 1)").hard,
            BTreeSet::from(["7".into()])
        );
        assert_eq!(key("Pokémon - Emerald Version").main, ["pokemon"]);
        assert_eq!(key("Ghouls & Ghosts").main, ["ghouls", "and", "ghosts"]);
        assert_eq!(key("Final Fantasy I").main, ["final", "fantasy", "1"]);
        assert_eq!(key("I Am Setsuna").main, ["i", "am", "setsuna"]);
    }

    #[test]
    fn number_titles_and_lone_letters() {
        assert_eq!(score("1942", "1942 (Japan, USA)"), 1.0);
        assert_eq!(score("1942", "1943 - The Battle of Midway (USA)"), 0.0);
        // A lone X is a letter, not 10.
        assert_eq!(score("Mega Man X (USA)", "Mega Man 10 (USA)"), 0.0);
        assert_eq!(score("Mega Man X", "Mega Man X (USA)"), 1.0);
        assert_eq!(key("X-Men (USA)").main, ["x", "men"]);
        assert_eq!(key("Final Fantasy VI (Japan)").hard, BTreeSet::from(["6".into()]));
    }

    #[test]
    fn numerals_and_editions_veto() {
        assert_eq!(score("Street Fighter II (USA)", "Street Fighter III"), 0.0);
        assert_eq!(score("Street Fighter II", "Street Fighter II Turbo"), 0.0);
        assert_eq!(score("Street Fighter 2", "Street Fighter II (Japan)"), 1.0);
        assert_eq!(score("Dragon Quest III", "Dragon Quest I & II (Japan)"), 0.0);
        assert_eq!(score("Pokemon - Gold Version", "Pokemon - Silver Version"), 0.0);
    }

    #[test]
    fn bands() {
        assert_eq!(
            score("Super Mario World (USA)", "Super Mario World (Europe) (Rev 1)"),
            1.0
        );
        let missing = score(
            "Dragon Quest III (English v2.0)[Addendum].sfc",
            "Dragon Quest III - Soshite Densetsu e... (Japan)",
        );
        assert_eq!(missing, 0.9);
        // "Ghouls'n" is "ghouls n": the same words.
        assert_eq!(score("Super Ghouls n Ghosts", "Super Ghouls'n Ghosts (USA)"), 1.0);
        // A typo stays below the exact bands.
        let typo = score("Super Ghoulss n Ghosts", "Super Ghouls'n Ghosts (USA)");
        assert_eq!(typo, 0.85, "every word, one with a typo");
        assert_eq!(
            score(
                "zelda link to the past",
                "Legend of Zelda, The - A Link to the Past (USA)"
            ),
            0.85
        );
        assert_eq!(
            score("Dragon Quest III", "Alien 3 (USA)"),
            0.0,
            "only the number in common"
        );
        assert!(
            score("Mario", "Super Mario World") < 0.85,
            "one word is not every word"
        );
        let other_sub = score(
            "Super Star Wars - Return of the Jedi",
            "Super Star Wars - The Empire Strikes Back",
        );
        assert!(other_sub > 0.0 && other_sub < 0.9, "{other_sub}");
        assert_eq!(score("Super Mario World", "Donkey Kong Country"), 0.0);
    }

    #[test]
    fn header_titles() {
        assert!(header_fits(
            "DRAGONQUEST3",
            "Dragon Quest III - Soshite Densetsu e... (Japan)"
        ));
        assert!(header_fits("SUPER MARIOWORLD", "Super Mario World (USA)"));
        assert!(header_fits(
            "THE LEGEND OF ZELDA",
            "Legend of Zelda, The - A Link to the Past (USA)"
        ));
        assert!(header_fits(
            "SUPER EMPIRE STRIKES",
            "Super Empire Strikes Back (Japan)"
        ));
        // The real header of Return of the Jedi: both its words are there.
        assert!(header_fits(
            "SUPER JEDI",
            "Super Star Wars - Return of the Jedi (USA)"
        ));
        assert!(!header_fits("SUPER JEDI", "Super Mario World (USA)"));
        assert!(!header_fits("DRAGONQUEST3", "Dragon Quest I & II (Japan)"));
        assert!(!header_fits("MARIO", "Mario Paint (USA)"), "too short to tell");
    }

    #[test]
    fn index_finds_by_shared_words() {
        let idx = TitleIndex::new(
            [
                "Dragon Quest III - Soshite Densetsu e... (Japan)",
                "Dragon Quest I & II (Japan)",
                "Dragon Quest V - Tenkuu no Hanayome (Japan)",
                "Super Mario World (USA)",
            ]
            .map(String::from),
        );
        let found = idx.find("Dragon Quest III (English v2.0)[Addendum]", 5);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, "Dragon Quest III - Soshite Densetsu e... (Japan)");
    }
}
