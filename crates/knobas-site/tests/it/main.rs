//! `knobas-site`'s integration tests, one binary (ADR-0017).
//!
//! - `scan`: the source scans that hold decisions §3 and §4;
//! - `sites`: the controller on the fake platform;
//! - `travel_focus`: the mouse's back and forward over a site, and the
//!   keyboard when the window comes back (#2528).

mod scan;
mod sites;
mod travel_focus;
