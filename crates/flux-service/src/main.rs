use std::convert::TryInto;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flux_schema::{fb, WIRE_SCHEMA_VERSION};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use zeromq::{Socket, SocketRecv, SocketSend};

#[derive(Debug, Clone)]
struct StreamKeyOwned {
    source_id: String,
    symbol: String,
    interval: String,
}

impl StreamKeyOwned {
    fn topic(&self) -> String {
        format!("candles.{}.{}.{}", self.source_id, self.symbol, self.interval)
    }
}

#[derive(Debug, Clone)]
struct CandleRow {
    sequence: u64,
    ts_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Debug)]
struct StreamState {
    key: StreamKeyOwned,
    start_ts_ms: i64,
    next_sequence: u64,
    candles: Vec<CandleRow>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let live_pub = std::env::var("FLUX_LIVE_PUB").unwrap_or_else(|_| "tcp://0.0.0.0:5556".into());
    let chunk_rep = std::env::var("FLUX_CHUNK_REP").unwrap_or_else(|_| "tcp://0.0.0.0:5557".into());

    let key = StreamKeyOwned {
        source_id: "SIM".to_string(),
        symbol: "TEST".to_string(),
        interval: "1s".to_string(),
    };

    let state = Arc::new(RwLock::new(StreamState {
        key: key.clone(),
        start_ts_ms: now_epoch_ms(),
        next_sequence: 1,
        candles: Vec::new(),
    }));

    let (pub_tx, mut pub_rx) = mpsc::channel::<(String, Vec<u8>)>(1024);

    let mut pub_socket = zeromq::PubSocket::new();
    pub_socket.bind(&live_pub).await?;
    println!("live_pub bound: {live_pub}");

    let mut rep_socket = zeromq::RepSocket::new();
    rep_socket.bind(&chunk_rep).await?;
    println!("chunk_rep bound: {chunk_rep}");

    let publisher_task = tokio::spawn(async move {
        while let Some((topic, payload)) = pub_rx.recv().await {
            let mut msg = zeromq::ZmqMessage::from(topic.as_str());
            msg.push_back(payload.into());
            if let Err(err) = pub_socket.send(msg).await {
                eprintln!("pub send error: {err}");
            }
        }
    });

    let state_for_gen = state.clone();
    let gen_task = tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let (topic, payload) = {
                let mut locked = state_for_gen.write().await;
                let candle = synth_next_candle(&mut locked);
                let payload = encode_candle_batch(&locked.key, candle.sequence, &[candle]);
                (locked.key.topic(), payload)
            };

            if pub_tx.send((topic, payload)).await.is_err() {
                break;
            }
        }
    });

    let state_for_rpc = state.clone();
    let rpc_task = tokio::spawn(async move {
        loop {
            let req = match rep_socket.recv().await {
                Ok(req) => req,
                Err(err) => {
                    eprintln!("rep recv error: {err}");
                    continue;
                }
            };

            let req_bytes: Result<Vec<u8>, _> = req.try_into();
            let req_bytes = match req_bytes {
                Ok(req_bytes) => req_bytes,
                Err(err) => {
                    let payload = encode_error(400, err);
                    let _ = rep_socket.send(payload.into()).await;
                    continue;
                }
            };

            let payload = handle_rpc(&req_bytes, &state_for_rpc).await;
            if let Err(err) = rep_socket.send(payload.into()).await {
                eprintln!("rep send error: {err}");
            }
        }
    });

    let _ = tokio::join!(publisher_task, gen_task, rpc_task);
    Ok(())
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn synth_next_candle(state: &mut StreamState) -> CandleRow {
    let sequence = state.next_sequence;
    state.next_sequence += 1;

    let ts_ms = state.start_ts_ms + (sequence as i64) * 1_000;
    let base = 100.0 + (sequence as f64) * 0.01;
    let drift = ((sequence % 10) as f64 - 5.0) * 0.02;
    let open = base;
    let close = base + drift;
    let high = open.max(close) + 0.05;
    let low = open.min(close) - 0.05;
    let volume = 1_000.0 + (sequence % 100) as f64;

    let row = CandleRow {
        sequence,
        ts_ms,
        open,
        high,
        low,
        close,
        volume,
    };
    state.candles.push(row.clone());
    row
}

async fn handle_rpc(req_bytes: &[u8], state: &Arc<RwLock<StreamState>>) -> Vec<u8> {
    let env = match fb::root_as_envelope(req_bytes) {
        Ok(env) => env,
        Err(_) => return encode_error(400, "invalid flatbuffer envelope"),
    };

    if env.schema_version() != WIRE_SCHEMA_VERSION {
        return encode_error(400, "unsupported schema_version");
    }

    let correlation_id = env.correlation_id().map(str::to_string);

    match env.message_type() {
        fb::Message::HealthRequest => {
            encode_health_response(correlation_id.as_deref(), "ok")
        }
        fb::Message::GetCursorRequest => {
            let req = match env.message_as_get_cursor_request() {
                Some(req) => req,
                None => return encode_error(400, "missing GetCursorRequest body"),
            };
            encode_cursor_response(correlation_id.as_deref(), req.key(), state).await
        }
        fb::Message::BackfillCandlesRequest => {
            let req = match env.message_as_backfill_candles_request() {
                Some(req) => req,
                None => return encode_error(400, "missing BackfillCandlesRequest body"),
            };
            encode_backfill_response(correlation_id.as_deref(), &req, state).await
        }
        _ => encode_error(404, "unsupported message_type"),
    }
}

async fn encode_cursor_response(
    correlation_id: Option<&str>,
    key: Option<fb::StreamKey<'_>>,
    state: &Arc<RwLock<StreamState>>,
) -> Vec<u8> {
    let requested = match key {
        Some(key) => key,
        None => return encode_error(400, "missing stream key"),
    };

    let locked = state.read().await;
    if !key_matches(&requested, &locked.key) {
        return encode_error(404, "unknown stream key");
    }

    let latest = locked.candles.last();
    let cursor = CursorValue {
        latest_sequence: latest.map(|c| c.sequence).unwrap_or(0),
        latest_ts_ms: latest.map(|c| c.ts_ms).unwrap_or(0),
    };
    encode_get_cursor_response(correlation_id, &locked.key, cursor)
}

async fn encode_backfill_response(
    correlation_id: Option<&str>,
    req: &fb::BackfillCandlesRequest<'_>,
    state: &Arc<RwLock<StreamState>>,
) -> Vec<u8> {
    let requested = match req.key() {
        Some(key) => key,
        None => return encode_error(400, "missing stream key"),
    };

    let locked = state.read().await;
    if !key_matches(&requested, &locked.key) {
        return encode_error(404, "unknown stream key");
    }

    let from_sequence = if req.has_from_sequence() {
        Some(req.from_sequence_exclusive())
    } else {
        None
    };
    let end_ts_ms = if req.has_end_ts_ms() {
        Some(req.end_ts_ms())
    } else {
        None
    };
    let limit = if req.limit() == 0 { 10_000 } else { req.limit() as usize };

    let mut out = Vec::new();
    for candle in &locked.candles {
        if let Some(from) = from_sequence {
            if candle.sequence <= from {
                continue;
            }
        }
        if let Some(end) = end_ts_ms {
            if candle.ts_ms > end {
                break;
            }
        }
        out.push(candle.clone());
        if out.len() >= limit {
            break;
        }
    }

    let has_more = {
        if out.is_empty() {
            false
        } else {
            let last_seq = out.last().map(|c| c.sequence).unwrap_or(0);
            locked.candles.last().map(|c| c.sequence).unwrap_or(0) > last_seq
        }
    };

    encode_backfill_candles_response(
        correlation_id,
        &locked.key,
        out.first().map(|c| c.sequence).unwrap_or(0),
        &out,
        has_more,
    )
}

fn key_matches(key: &fb::StreamKey<'_>, owned: &StreamKeyOwned) -> bool {
    key.source_id() == Some(owned.source_id.as_str())
        && key.symbol() == Some(owned.symbol.as_str())
        && key.interval() == Some(owned.interval.as_str())
}

fn encode_health_response(correlation_id: Option<&str>, message: &str) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let message = fbb.create_string(message);
    let health = fb::HealthResponse::create(
        &mut fbb,
        &fb::HealthResponseArgs {
            ok: true,
            message: Some(message),
        },
    );
    let envelope = build_envelope(
        &mut fbb,
        fb::MessageType::HEALTH_RESPONSE,
        fb::Message::HealthResponse,
        health.as_union_value(),
        correlation_id,
    );
    fb::finish_envelope_buffer(&mut fbb, envelope);
    fbb.finished_data().to_vec()
}

#[derive(Debug, Clone, Copy)]
struct CursorValue {
    latest_sequence: u64,
    latest_ts_ms: i64,
}

fn encode_get_cursor_response(
    correlation_id: Option<&str>,
    key: &StreamKeyOwned,
    cursor: CursorValue,
) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let key = build_stream_key(&mut fbb, key);
    let cursor = fb::Cursor::create(
        &mut fbb,
        &fb::CursorArgs {
            latest_sequence: cursor.latest_sequence,
            latest_ts_ms: cursor.latest_ts_ms,
        },
    );
    let resp = fb::GetCursorResponse::create(
        &mut fbb,
        &fb::GetCursorResponseArgs {
            key: Some(key),
            cursor: Some(cursor),
        },
    );
    let envelope = build_envelope(
        &mut fbb,
        fb::MessageType::GET_CURSOR_RESPONSE,
        fb::Message::GetCursorResponse,
        resp.as_union_value(),
        correlation_id,
    );
    fb::finish_envelope_buffer(&mut fbb, envelope);
    fbb.finished_data().to_vec()
}

fn encode_candle_batch(key: &StreamKeyOwned, start_sequence: u64, candles: &[CandleRow]) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let key = build_stream_key(&mut fbb, key);

    let mut candle_offsets = Vec::with_capacity(candles.len());
    for candle in candles {
        candle_offsets.push(fb::Candle::create(
            &mut fbb,
            &fb::CandleArgs {
                ts_ms: candle.ts_ms,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                volume: candle.volume,
            },
        ));
    }
    let candles = fbb.create_vector(&candle_offsets);

    let batch = fb::CandleBatch::create(
        &mut fbb,
        &fb::CandleBatchArgs {
            key: Some(key),
            start_sequence,
            candles: Some(candles),
        },
    );

    let envelope = build_envelope(
        &mut fbb,
        fb::MessageType::CANDLE_BATCH,
        fb::Message::CandleBatch,
        batch.as_union_value(),
        None,
    );
    fb::finish_envelope_buffer(&mut fbb, envelope);
    fbb.finished_data().to_vec()
}

fn encode_backfill_candles_response(
    correlation_id: Option<&str>,
    key: &StreamKeyOwned,
    start_sequence: u64,
    candles: &[CandleRow],
    has_more: bool,
) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let key = build_stream_key(&mut fbb, key);

    let mut candle_offsets = Vec::with_capacity(candles.len());
    for candle in candles {
        candle_offsets.push(fb::Candle::create(
            &mut fbb,
            &fb::CandleArgs {
                ts_ms: candle.ts_ms,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                volume: candle.volume,
            },
        ));
    }
    let candles_vec = fbb.create_vector(&candle_offsets);

    let resp = fb::BackfillCandlesResponse::create(
        &mut fbb,
        &fb::BackfillCandlesResponseArgs {
            key: Some(key),
            start_sequence,
            candles: Some(candles_vec),
            has_more,
            next_sequence: start_sequence + candles.len() as u64,
        },
    );

    let envelope = build_envelope(
        &mut fbb,
        fb::MessageType::BACKFILL_CANDLES_RESPONSE,
        fb::Message::BackfillCandlesResponse,
        resp.as_union_value(),
        correlation_id,
    );
    fb::finish_envelope_buffer(&mut fbb, envelope);
    fbb.finished_data().to_vec()
}

fn encode_error(code: u32, message: &str) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let message = fbb.create_string(message);
    let err = fb::ErrorResponse::create(
        &mut fbb,
        &fb::ErrorResponseArgs {
            code,
            message: Some(message),
        },
    );
    let envelope = build_envelope(
        &mut fbb,
        fb::MessageType::ERROR_RESPONSE,
        fb::Message::ErrorResponse,
        err.as_union_value(),
        None,
    );
    fb::finish_envelope_buffer(&mut fbb, envelope);
    fbb.finished_data().to_vec()
}

fn build_stream_key<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    key: &StreamKeyOwned,
) -> flatbuffers::WIPOffset<fb::StreamKey<'a>> {
    let source_id = fbb.create_string(&key.source_id);
    let symbol = fbb.create_string(&key.symbol);
    let interval = fbb.create_string(&key.interval);
    fb::StreamKey::create(
        fbb,
        &fb::StreamKeyArgs {
            source_id: Some(source_id),
            symbol: Some(symbol),
            interval: Some(interval),
        },
    )
}

fn build_envelope<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    type_hint: fb::MessageType,
    message_type: fb::Message,
    message: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    correlation_id: Option<&str>,
) -> flatbuffers::WIPOffset<fb::Envelope<'a>> {
    let correlation_id = correlation_id.map(|s| fbb.create_string(s));
    fb::Envelope::create(
        fbb,
        &fb::EnvelopeArgs {
            schema_version: WIRE_SCHEMA_VERSION,
            type_hint,
            correlation_id,
            message_type,
            message: Some(message),
        },
    )
}
