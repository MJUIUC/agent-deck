// lib.rs — re-exports all internal crates for integration tests in tests/
#![recursion_limit = "256"]

pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod services;
