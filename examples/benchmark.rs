//! Benchmark — prove SuperFAST is *super* fast.
//!
//! Measures encode throughput, decode throughput, round-trip latency,
//! and compression ratios over realistic market-data message sequences.
//!
//! ```bash
//! cargo run --release --example benchmark
//! ```

use std::time::Instant;
use superfast::decimal::Decimal;
use superfast::{Dictionary, FastDecoder, FastEncoder};

// ---------------------------------------------------------------------------
// Schema + types (shared with demo.rs)
// ---------------------------------------------------------------------------

const SCHEMA: &str = include_str!("schema.xml");

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename = "MarketData")]
struct MarketData {
    #[serde(rename = "Symbol")]
    symbol: String,
    #[serde(rename = "SequenceNumber")]
    sequence_number: u64,
    #[serde(rename = "Price")]
    price: Decimal,
    #[serde(rename = "Volume", default)]
    volume: Option<u32>,
    #[serde(rename = "Side", default)]
    side: Option<i32>,
    #[serde(rename = "Exchange", default)]
    exchange: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename = "TradeCapture")]
struct TradeCapture {
    #[serde(rename = "TradeID")]
    trade_id: u32,
    #[serde(rename = "Symbol")]
    symbol: String,
    #[serde(rename = "Price")]
    price: Decimal,
    #[serde(rename = "Quantity")]
    quantity: u64,
    #[serde(rename = "Timestamp", default)]
    timestamp: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)] // we only deserialize into this enum, never read the inner fields
enum Message {
    #[serde(rename = "MarketData")]
    MarketData(MarketData),
    #[serde(rename = "TradeCapture")]
    TradeCapture(TradeCapture),
}

// ---------------------------------------------------------------------------
// Message generators — realistic market data patterns
// ---------------------------------------------------------------------------

/// Generate N market-data messages for a single symbol with slowly changing fields
/// (maximizes compression from copy/increment/tail operators).
fn gen_steady_stream(count: usize) -> Vec<MarketData> {
    let symbols = [
        "AAPL", "GOOG", "MSFT", "AMZN", "TSLA", "NVDA", "META", "JPM",
    ];
    let exchanges = ["NASDAQ", "NYSE", "ARCA", "BATS", "IEX"];

    symbols
        .into_iter()
        .cycle()
        .zip(1u64..)
        .take(count)
        .map(|(sym, seq)| {
            let base_price = 150.0 + (seq as f64 % 500.0);
            let price = Decimal::from_float(base_price).unwrap();
            MarketData {
                symbol: sym.to_string(),
                sequence_number: seq,
                price,
                volume: Some((seq * 7 % 10000) as u32),
                side: Some((seq % 2) as i32),
                exchange: Some(exchanges[(seq as usize) % exchanges.len()].to_string()),
            }
        })
        .collect()
}

/// Generate N market-data messages where every field changes every time
/// (worst case — no compression from stateful operators).
fn gen_churning_stream(count: usize) -> Vec<MarketData> {
    (0u64..)
        .map(|i| {
            let sym_idx = (i % 20) as u32;
            let price = Decimal::from_float(100.0 + (i as f64 * 0.137)).unwrap();
            MarketData {
                symbol: format!("SYM{i:04}"),
                sequence_number: i,
                price,
                volume: Some((i * 31 % 99999) as u32),
                side: Some((i % 4) as i32),
                exchange: Some(format!("EX{sym_idx}")),
            }
        })
        .take(count)
        .collect()
}

/// Generate N mixed TradeCapture messages.
fn gen_trades(count: usize) -> Vec<TradeCapture> {
    let symbols = ["AAPL", "GOOG", "MSFT", "AMZN", "TSLA"];
    symbols
        .into_iter()
        .cycle()
        .zip(1u32..)
        .take(count)
        .map(|(sym, id)| TradeCapture {
            trade_id: id,
            symbol: sym.to_string(),
            price: Decimal::from_float(100.0 + (id as f64 * 0.073)).unwrap(),
            quantity: (id as u64 * 17 % 10000) + 1,
            timestamp: Some(format!(
                "2024-01-15T10:{:02}:{:02}",
                (id / 60) % 60,
                id % 60
            )),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmark harness
// ---------------------------------------------------------------------------

struct BenchResult {
    name: &'static str,
    messages: usize,
    total_bytes: usize,
    elapsed_ms: f64,
}

impl BenchResult {
    fn msgs_per_sec(&self) -> f64 {
        (self.messages as f64) / (self.elapsed_ms / 1000.0)
    }

    fn mb_per_sec(&self) -> f64 {
        (self.total_bytes as f64 / 1_048_576.0) / (self.elapsed_ms / 1000.0)
    }

    fn us_per_msg(&self) -> f64 {
        (self.elapsed_ms * 1000.0) / (self.messages as f64)
    }

    fn print(&self) {
        println!(
            "  {:<26} {:>10} msgs  {:>8} bytes  {:>8.1} ms",
            self.name, self.messages, self.total_bytes, self.elapsed_ms
        );
        println!(
            "                      {:>10.0} msgs/s  {:>8.2} MB/s  {:>8.0} µs/msg",
            self.msgs_per_sec(),
            self.mb_per_sec(),
            self.us_per_msg()
        );
    }
}

fn bench_encode<T>(name: &'static str, items: &[T]) -> (BenchResult, Vec<u8>)
where
    T: serde::Serialize,
{
    let mut enc = FastEncoder::new(SCHEMA, Dictionary::Global).unwrap();

    // Warmup — encode enough to boost CPU frequency before timing
    for item in &items[..1000] {
        let _ = enc.encode(item).unwrap();
    }

    let start = Instant::now();
    let mut all_bytes = Vec::new();
    for item in items {
        all_bytes.extend(enc.encode(item).unwrap());
    }
    let elapsed = start.elapsed();

    let result = BenchResult {
        name,
        messages: items.len(),
        total_bytes: all_bytes.len(),
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
    };
    (result, all_bytes)
}

fn bench_decode(name: &'static str, bytes: &[u8], msg_count: usize) -> BenchResult {
    let mut dec = FastDecoder::new(SCHEMA, Dictionary::Global).unwrap();

    // Warmup — decode enough to boost CPU frequency before timing
    let mut warmup_offset = 0;
    for _ in 0..1000 {
        if warmup_offset >= bytes.len() {
            break;
        }
        let (_msg, consumed): (Message, u64) = dec.decode(&bytes[warmup_offset..]).unwrap();
        warmup_offset += consumed as usize;
    }

    let start = Instant::now();
    let mut offset = 0;
    let mut decoded = 0;
    while offset < bytes.len() && decoded < msg_count {
        let (_msg, consumed): (Message, u64) = dec.decode(&bytes[offset..]).unwrap();
        offset += consumed as usize;
        decoded += 1;
    }
    let elapsed = start.elapsed();

    BenchResult {
        name,
        messages: decoded,
        total_bytes: bytes.len(),
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
    }
}

fn bench_roundtrip<T>(name: &'static str, items: &[T]) -> BenchResult
where
    T: serde::Serialize,
{
    let mut enc = FastEncoder::new(SCHEMA, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(SCHEMA, Dictionary::Global).unwrap();

    // Warmup — boost CPU frequency before timing
    for item in &items[..1000] {
        let encoded = enc.encode(item).unwrap();
        let _ = dec.decode::<Message>(&encoded[..]).unwrap();
    }

    let start = Instant::now();
    let mut all_bytes = Vec::new();
    for item in items {
        all_bytes.extend(enc.encode(item).unwrap());
    }
    let mut offset = 0;
    let mut decoded = 0;
    while offset < all_bytes.len() && decoded < items.len() {
        let (_msg, consumed): (Message, u64) = dec.decode(&all_bytes[offset..]).unwrap();
        offset += consumed as usize;
        decoded += 1;
    }
    let elapsed = start.elapsed();

    BenchResult {
        name,
        messages: decoded,
        total_bytes: all_bytes.len(),
        elapsed_ms: elapsed.as_secs_f64() * 1000.0,
    }
}

fn bench_compression<T>(name: &'static str, items: &[T])
where
    T: serde::Serialize,
{
    let mut enc = FastEncoder::new(SCHEMA, Dictionary::Global).unwrap();

    // First message — full payload (no prior context)
    let first_bytes = enc.encode(&items[0]).unwrap();
    let first_size = first_bytes.len();

    // Second message — some context available
    let second_bytes = enc.encode(&items[1]).unwrap();
    let second_size = second_bytes.len();

    // 50th message — context fully warmed
    for i in 2..50 {
        let _ = enc.encode(&items[i]).unwrap();
    }
    let fiftieth_bytes = enc.encode(&items[49]).unwrap();
    let fiftieth_size = fiftieth_bytes.len();

    // Last message — maximum context utilization
    let last_bytes = enc.encode(items.last().unwrap()).unwrap();
    let last_size = last_bytes.len();

    println!(
        "  {:<30} 1st: {:>3} bytes  2nd: {:>3} bytes  50th: {:>3} bytes  last: {:>3} bytes",
        name, first_size, second_size, fiftieth_size, last_size
    );
    println!(
        "                            2nd: {:>5.0}%  50th: {:>5.0}%  last: {:>5.0}%  (of first)",
        second_size as f64 / first_size as f64 * 100.0,
        fiftieth_size as f64 / first_size as f64 * 100.0,
        last_size as f64 / first_size as f64 * 100.0,
    );
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const N: usize = 100_000;

fn main() {
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║  SuperFAST Benchmark — FAST v1.1 Encoder/Decoder                  ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    // Generate test data
    let steady = gen_steady_stream(N);
    let churning = gen_churning_stream(N);
    let trades = gen_trades(N);

    // ── Encode throughput ──────────────────────────────────────────
    println!("┌── Encode Throughput ──────────────────────────────────────────────┐");
    println!("│");

    let (enc_steady, steady_bytes) = bench_encode("encode steady stream", &steady);
    enc_steady.print();
    println!();

    let (enc_churning, churning_bytes) = bench_encode("encode churning stream", &churning);
    enc_churning.print();
    println!();

    let (enc_trades, trade_bytes) = bench_encode("encode trades", &trades);
    enc_trades.print();
    println!("│");
    println!("╞════════════════════════════════════════════════════════════════════╡");

    // ── Decode throughput ──────────────────────────────────────────
    println!("┌── Decode Throughput ──────────────────────────────────────────────┐");
    println!("│");

    let dec_steady = bench_decode("decode steady stream", &steady_bytes, N);
    dec_steady.print();
    println!();

    let dec_churning = bench_decode("decode churning stream", &churning_bytes, N);
    dec_churning.print();
    println!();

    let dec_trades = bench_decode("decode trades", &trade_bytes, N);
    dec_trades.print();
    println!("│");
    println!("╞════════════════════════════════════════════════════════════════════╡");

    // ── Round-trip latency ─────────────────────────────────────────
    println!("┌── Round-Trip Latency (encode + decode) ───────────────────────────┐");
    println!("│");

    let rt_steady = bench_roundtrip("round-trip steady", &steady);
    rt_steady.print();
    println!();

    let rt_churning = bench_roundtrip("round-trip churning", &churning);
    rt_churning.print();
    println!();

    let rt_trades = bench_roundtrip("round-trip trades", &trades);
    rt_trades.print();
    println!("│");
    println!("╞════════════════════════════════════════════════════════════════════╡");

    // ── Compression ratios ─────────────────────────────────────────
    println!("┌── Compression (stateful operators) ───────────────────────────────┐");
    println!("│");

    let steady_100 = gen_steady_stream(100);
    bench_compression("steady (copy/inc/tail)", &steady_100);
    println!();

    let churning_100 = gen_churning_stream(100);
    bench_compression("churning (all change)", &churning_100);
    println!();

    let trades_100 = gen_trades(100);
    bench_compression("trades (TradeCapture)", &trades_100);
    println!("│");
    println!("╚════════════════════════════════════════════════════════════════════╝");
}
