//! Native twin of the wasm probe.
//!
//! Prints exactly what `devch_probe_write` copies out of wasm memory, so the two
//! builds can be compared byte for byte. Any difference means the wasm and native
//! code paths diverge, which would disqualify sharing the Core with the browser
//! (ADR-0001).

fn main() {
    print!("{}", devchievements_core::wasm_probe::sample_json());
}
