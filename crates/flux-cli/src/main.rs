use flux_core::{Symbol, TimeRange};
use flux_db::{connection_string, plan_tick_window, DbConfig};

fn main() {
    let symbol = Symbol::from("CLI");
    let range = TimeRange::new(0, 60);
    let db_cfg = DbConfig::new("postgres://localhost/flux", "flux-cli");

    println!("{} planning export", connection_string(&db_cfg));
    println!("chunk preview: {}", plan_tick_window(&symbol, &range));
}
