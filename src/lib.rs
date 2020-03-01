#![recursion_limit = "256"]
#![deny(nonstandard_style, future_incompatible, clippy::all, clippy::restriction, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::multiple_inherent_impl,
    clippy::implicit_return,
    clippy::missing_inline_in_public_items,
    clippy::missing_docs_in_private_items
)]

//! Rust bindings to libzfs_core and wrapper around `zpool(8)`.
//!
//! This library intends to provide a safe, low-level interface to ZFS operator tools. As such, not
//! much will be sugar coated here.
//!
//! # Overview
//! ## zpool
//! A feature complete wrapper around `zpool(8)` with a somewhat stable API. I can't
//! guarantee that the API won't change at any moment, but I don't see a reason for it change at the
//! moment.
//!
//! Refer to the [zpool module documentation](zpool/index.html) for more information.
//!
//! ## zfs
//! Work on bindings to `libzfs_core` is just starting, so support for it is non-existent at the
//! moment.
//!
//! # Usage
//! Right now there are no "library usage" instructions, but the zpool module can be used directly.
//! In the future some sugar to setup logging will be added to the library level.
//!
//! # Project Structure
//! ### parsers
//! Module for PEG parsers backed by [Pest](https://pest.rs/).
//!
//! ### zpool
//! This module contains everything you need to work with zpools.

#[macro_use] extern crate derive_builder;
#[macro_use] extern crate getset;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate quick_error;
use strum;

#[macro_use] pub extern crate slog;
pub use pest;

// library modules
pub mod parsers;
pub mod zfs;
pub mod zpool;

pub mod utils;

#[cfg(fuzzing)] pub mod fuzzy;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod log;
pub use log::Logger;
