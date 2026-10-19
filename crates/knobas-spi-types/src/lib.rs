//! The types the source SPI shares with the core (#1234), in a crate that
//! builds for `wasm32-wasip2`: what a payload declaration says
//! ([`payload`]), how an entity is named ([`entity`]), the tone a value
//! is drawn in ([`tone`]), and how a write-back op is delivered
//! ([`delivery`], ADR-0006 as amended 2026-10-19).
//!
//! They lived in `knobas-core`, which needs sqlx and so cannot reach a WASM
//! component. `knobas-core` re-exports every module here, and the payload
//! SQL macros, under their old paths, so no caller changed.

pub mod delivery;
pub mod entity;
pub mod payload;
pub mod tone;
