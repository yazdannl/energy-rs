//! Price providers.
//!
//! `ZonneplanDirect` reads the direct API with a stored refresh token; `HaSensors` reads the
//! integration's sensor attributes as a fallback. Both return the same slot vector, and a failed
//! refresh keeps the previous publication instead of inventing numbers.
//!
//! No code yet: P0 scaffold only.
