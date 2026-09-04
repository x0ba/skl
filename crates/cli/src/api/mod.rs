//! Full locked `/v1` surface. Login/init/sync call a subset; the rest is for
//! hammer / the next pass.
#![allow(dead_code)]

pub mod client;
pub mod types;

pub use client::ApiClient;
pub use types::*;
