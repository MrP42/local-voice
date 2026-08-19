// M8 fundament: this module defines the storage interface that later meeting
// tasks (recording, live transcription, minutes generation, cleanup) build
// on. Until those tasks wire it into commands, several items here have no
// caller yet — expected for a store-only milestone, not dead code to prune.
#![allow(dead_code, unused_imports)]

pub mod chunker;
pub mod import;
pub mod mic_capture;
pub mod minutes;
pub mod recorder;
pub mod stats;
pub mod store;
pub mod subtitle;
pub use store::*;
