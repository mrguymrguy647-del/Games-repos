//! LUCKY WORLD: spin 16 wheels for your country, meet your rivals.

mod country;
mod data;
mod fate;
#[cfg(test)]
mod tests;

use duckforge::prelude::*;

/// Wheel ids the code refers to by name (tests check wheels.toml has them).
pub const WHEELS_THE_CODE_NAMES: [&str; 8] = ["continent", "size", "coast", "population", "ideology", "gdp", "army", "duty"];

struct G;
impl Game for G {
    fn update(&mut self, _c: &mut Context) {}
    fn draw(&self, c: &mut Canvas) {
        c.clear(Color::BLACK);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = data::Data::load()?;
    duckforge::run(Config::default(), G)
}
