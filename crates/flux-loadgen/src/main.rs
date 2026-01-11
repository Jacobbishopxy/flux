use flux_schema::fb;
use zeromq::{Socket, SocketRecv};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let live_pub = std::env::var("FLUX_LIVE_PUB").unwrap_or_else(|_| "tcp://127.0.0.1:5556".into());
    let topic = std::env::var("FLUX_TOPIC").unwrap_or_else(|_| "candles.SIM.TEST.1s".into());

    let mut socket = zeromq::SubSocket::new();
    socket.connect(&live_pub).await?;
    socket.subscribe(&topic).await?;

    println!("sub connected: {live_pub}");
    println!("sub topic: {topic}");

    loop {
        let msg = socket.recv().await?;
        if msg.len() < 2 {
            continue;
        }

        let topic = msg.get(0).and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("");
        let payload = msg.get(1).map(|b| b.as_ref()).unwrap_or(&[]);

        let env = match fb::root_as_envelope(payload) {
            Ok(env) => env,
            Err(_) => {
                println!("recv topic={topic} (invalid envelope, {} bytes)", payload.len());
                continue;
            }
        };

        if env.message_type() == fb::Message::CandleBatch {
            if let Some(batch) = env.message_as_candle_batch() {
                let count = batch.candles().map(|c| c.len()).unwrap_or(0);
                println!(
                    "recv topic={topic} seq={} count={}",
                    batch.start_sequence(),
                    count
                );
            }
        } else {
            println!("recv topic={topic} type={:?}", env.message_type());
        }
    }
}
