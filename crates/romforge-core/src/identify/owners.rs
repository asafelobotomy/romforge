//! Who owns each ROM, and what that makes each file: the part of
//! [`super::identify`] that is the rules alone, with no database.

use std::collections::{HashMap, HashSet};

use super::{Hit, Status};

/// Each ROM is owned by the copy whose place (an archive or a folder)
/// holds the most of that ROM's game; the other copies are duplicates.
/// Ties go to the first file (files come by path). Per ROM, not per file:
/// a track two discs share belongs, on each disc, to the copy beside that
/// disc's other tracks. `places[i]` is file `i`'s, `hits[i]` its hits.
/// Returns ROM id -> (how much of its game the owner's place holds, owner).
pub(super) fn owners(places: &[&str], hits: &[Vec<Hit>]) -> HashMap<i64, (usize, usize)> {
    let strong = |h: &&Hit| h.in_scope && h.via.strong();
    let mut held: HashMap<(&str, i64), HashSet<i64>> = HashMap::new();
    for (place, hits) in places.iter().zip(hits) {
        for h in hits.iter().filter(strong) {
            held.entry((place, h.game_id)).or_default().insert(h.rom_id);
        }
    }
    let mut owners: HashMap<i64, (usize, usize)> = HashMap::new();
    for (i, (place, hits)) in places.iter().zip(hits).enumerate() {
        for h in hits.iter().filter(strong) {
            let score = held.get(&(*place, h.game_id)).map_or(0, HashSet::len);
            let best = owners.entry(h.rom_id).or_insert((score, i));
            if score > best.0 {
                *best = (score, i);
            }
        }
    }
    owners
}

/// File `i`'s status from its `hits` and the ROMs' `owners`; `named`: the
/// owner named its content in Search (house rule 8: a strong match in
/// scope comes first, and a name given by hand is never verified).
pub(super) fn status_of(
    i: usize,
    hits: &[Hit],
    owners: &HashMap<i64, (usize, usize)>,
    named: bool,
) -> Status {
    let strong_in_scope: Vec<i64> = hits
        .iter()
        .filter(|h| h.in_scope && h.via.strong())
        .map(|h| h.rom_id)
        .collect();
    if !strong_in_scope.is_empty() {
        if strong_in_scope
            .iter()
            .any(|r| owners.get(r).is_some_and(|o| o.1 == i))
        {
            Status::Matched
        } else {
            Status::Duplicate
        }
    } else if named {
        // The owner's pick outranks weak evidence (a CRC32-only hit, a
        // hash hit outside the folder's scope).
        Status::Manual
    } else if hits.iter().any(|h| h.in_scope) {
        Status::Candidate
    } else if hits.iter().any(|h| h.via.strong()) {
        Status::WrongSystem
    } else {
        Status::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::super::Via;
    use super::*;

    fn hit(rom_id: i64, game_id: i64, via: Via, in_scope: bool) -> Hit {
        Hit {
            rom_id,
            game_id,
            via,
            stripped: false,
            in_scope,
        }
    }

    #[test]
    fn the_copy_holding_more_of_the_game_owns_its_roms() {
        // Two tracks of game 1: both in disc A's folder, one also loose.
        let places = ["/a", "/a", "/loose"];
        let hits = vec![
            vec![hit(10, 1, Via::Sha1, true)],
            vec![hit(11, 1, Via::Sha1, true)],
            vec![hit(10, 1, Via::Sha1, true)],
        ];
        let o = owners(&places, &hits);
        assert_eq!(o[&10], (2, 0), "the folder with both tracks");
        assert_eq!(status_of(0, &hits[0], &o, false), Status::Matched);
        assert_eq!(status_of(2, &hits[2], &o, false), Status::Duplicate);
    }

    #[test]
    fn weak_and_hand_named_files_are_never_verified() {
        let o = HashMap::new();
        let crc = [hit(10, 1, Via::Crc32, true)];
        assert_eq!(status_of(0, &crc, &o, false), Status::Candidate, "CRC32 alone");
        assert_eq!(status_of(0, &crc, &o, true), Status::Manual, "named by you");
        let elsewhere = [hit(10, 1, Via::Sha1, false)];
        assert_eq!(status_of(0, &elsewhere, &o, false), Status::WrongSystem);
        assert_eq!(status_of(0, &[], &o, false), Status::Unknown);
        let o = owners(&["/a"], &[vec![hit(10, 1, Via::Sha1, true)]]);
        let strong = [hit(10, 1, Via::Sha1, true)];
        assert_eq!(
            status_of(0, &strong, &o, true),
            Status::Matched,
            "a strong match comes first"
        );
    }
}
