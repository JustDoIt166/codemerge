#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod application;
pub mod domain;
pub mod error;
pub mod processor;
pub mod services;
pub mod ui;
pub mod utils;
