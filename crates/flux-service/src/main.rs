use flux_core::{crate_tag as core_tag, Symbol, TimeRange};
use flux_db::{connection_string, plan_tick_window, DbConfig};
use flux_io::{backfill_envelope, crate_tag as io_tag, subscription_label, ZmqEndpoint};
use flux_schema::crate_tag as schema_tag;

fn main() {
    let symbol = Symbol::from("TEST");
    let range = TimeRange::new(1_700_000_000, 1_700_000_100);
    let endpoint = ZmqEndpoint::new("tcp://0.0.0.0:5555", "market.demo");
    let db_cfg = DbConfig::new("postgres://localhost/flux", "flux-service");

    println!(
        "booting {} with {} and {} support",
        core_tag(),
        schema_tag(),
        io_tag()
    );
    println!("connecting via {}", connection_string(&db_cfg));
    println!("ingest route: {}", subscription_label(&symbol, &endpoint));
    println!("backfill plan: {}", plan_tick_window(&symbol, &range));
    println!(
        "envelope preview: {}",
        backfill_envelope(range).topic_hint()
    );
}
