//! Oxy is a remote access tool designed to resist man-in-the-middle attacks
//! irrespective of user dilligence. This is an experimental version. Do not
//! use outside of an isolated lab environment.

#![warn(missing_docs)]
#![feature(int_to_from_bytes)]

#[macro_use]
extern crate serde_derive;

pub mod arg;
pub mod config;
pub mod entry;
pub mod oxy;

mod base32;
mod config_wizard;
mod inner;
mod innermessage;
mod log;
mod messagewatchers;
mod mid;
mod outer;
mod oxyhelpers;
mod pty;
mod ui;

pub(crate) const NOISE_PATTERN: &str = "Noise_IK_25519_AESGCM_SHA512";
pub(crate) const PROTOCOL_VERSION: u64 = 0;
