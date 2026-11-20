//! # `app-server`
//!
//! The main application server binary. It wires the business modules under
//! `modules/` together and runs them behind one or more *workers*.
//!
//! A worker is a single run mode selected at startup (typically via environment
//! variables). Common modes are:
//!
//! - a **gRPC** server exposing each module's `rpc` services,
//! - an **Iggy consumer** driving each module's event `hooks`,
//! - a **cron executor** running each module's `CronJob`s,
//! - a **REST/webhook** gateway for third-party callbacks.
//!
//! Shipping one binary that can run in several modes keeps deployment uniform
//! while letting each responsibility scale independently.
//!
//! See `bin/app-server/README.md` for the full description.

fn main() {
    println!("Hello, world!");
}
