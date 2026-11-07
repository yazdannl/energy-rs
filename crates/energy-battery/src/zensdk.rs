//! zenSDK HTTP transport.
//!
//! Plain HTTP on port 80, one host per device, no authentication. Charge is
//! `{smartMode:1, acMode:1, outputLimit:0, inputLimit:W}`, discharge mirrors it, and a hold sets
//! both limits to 0. Echo confirmation, pacing and command serialization live here.
//!
//! No code yet: P0 scaffold only.
