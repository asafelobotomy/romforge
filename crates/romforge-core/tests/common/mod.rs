//! Helpers shared by the integration tests. Each test binary compiles all
//! of them and uses some.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub mod cleanup;
pub mod fixture_data;
pub mod fixture_lib;
pub mod home;
pub mod organize;
pub mod patcher;
pub mod search;
pub mod serve;

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}
