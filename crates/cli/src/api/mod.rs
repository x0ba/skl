//! Full locked `/v1` surface from `apps/api/src/contracts.ts`.
//! Unused client methods stay so the crate mirrors every `/v1` route.
#![allow(dead_code)]

pub mod client;
pub mod types;

pub use client::ApiClient;
pub use types::*;
