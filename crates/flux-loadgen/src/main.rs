use flux_core::Symbol;
use flux_io::{subscription_label, ZmqEndpoint};

fn main() {
    let endpoint = ZmqEndpoint::new("tcp://localhost:6000", "backtest.demo");
    let symbol = Symbol::from("LOAD");

    println!(
        "load generator targeting {}",
        subscription_label(&symbol, &endpoint)
    );
}
