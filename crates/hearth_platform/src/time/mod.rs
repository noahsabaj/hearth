//! Provides `Instant` for all platforms.

pub use time::Instant;

crate::cfg::switch! {
    crate::cfg::web => {
        use web_time as time;
    }
    crate::cfg::axiom => {
        // Axiom's `std` has no clock, so Axiom's platform crate gives `Instant` one through
        // `Instant::set_elapsed`.
        mod fallback;

        use fallback as time;
    }
    crate::cfg::std => {
        use std::time;
    }
    _ => {
        mod fallback;

        use fallback as time;
    }
}
