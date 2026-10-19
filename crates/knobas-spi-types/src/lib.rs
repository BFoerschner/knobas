//! The types the source SPI shares with the core (#1234), in a crate that
//! builds for `wasm32-wasip2`: what a payload declaration says
//! ([`payload`]), how an entity is named ([`entity`]), and the tone a value
//! is drawn in ([`tone`]).
//!
//! They lived in `knobas-core`, which needs sqlx and so cannot reach a WASM
//! component. `knobas-core` re-exports all three modules, and the payload
//! SQL macros, under their old paths, so no caller changed.

pub mod entity;
pub mod payload;
pub mod tone;
