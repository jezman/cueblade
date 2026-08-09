//! Audio codec abstraction layer.
//!
//! Provides format-agnostic [`Decoder`] and [`Encoder`] traits
//! for lossless audio processing. All sample arithmetic uses
//! checked operations to prevent overflow (SECURITY.md).

pub mod factory;
pub mod flac;
pub mod traits;
pub mod types;

pub use factory::open_decoder;
pub use traits::{Decoder, Encoder};
pub use types::AudioInfo;
