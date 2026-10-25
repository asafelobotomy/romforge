//! System keys written in the code outside the systems table (house rule
//! 4), checked against it when the crate builds.

use super::SYSTEMS;

/// `key`, checked to be a system's key of the table: through [`key!`], at
/// compile time, so a misspelt key stops the build.
pub const fn checked(key: &'static str) -> &'static str {
    let mut i = 0;
    while i < SYSTEMS.len() {
        let k = SYSTEMS[i].key.as_bytes();
        let want = key.as_bytes();
        if k.len() == want.len() {
            let mut j = 0;
            while j < k.len() && k[j] == want[j] {
                j += 1;
            }
            if j == k.len() {
                return key;
            }
        }
        i += 1;
    }
    panic!("not a system key of systems/table.rs");
}

/// A system's key written in the code outside the systems table, checked
/// against it when the crate builds: `key!("GameCube")`.
macro_rules! key {
    ($k:literal) => {{
        const K: &str = $crate::systems::checked($k);
        K
    }};
}
pub(crate) use key;
