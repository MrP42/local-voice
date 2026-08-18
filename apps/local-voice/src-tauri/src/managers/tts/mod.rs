//! Vorlesen (TP1): Fish-Speech-TTS-Anbindung.
//!
//! `protocol` und `state` sind pure, I/O-freie Bausteine; der eigentliche
//! Manager (Prozess, HTTP, Playback) folgt in den nächsten Tasks.

pub mod protocol;
