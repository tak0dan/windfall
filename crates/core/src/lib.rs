//! Shared brains of Windfall: settings, the control protocol and the particle/flower simulation.
//! Nothing in here touches Wayland or the GPU, so all of it is unit-testable.

pub mod config;
pub mod draw;
pub mod geom;
pub mod ipc;
pub mod palette;
pub mod rng;
pub mod shape;
pub mod sim;
