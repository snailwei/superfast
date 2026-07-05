//! Demonstrates all README examples for SuperFAST:
//! schema embedding, struct definitions, encoding, decoding,
//! multi-template enums, decimal usage, and context management.

use superfast::decimal::Decimal;
use superfast::{FastDecoder, FastEncoder};

// ---------------------------------------------------------------------------
// 1. Embed Your XML Schema with `include_str!`
// ---------------------------------------------------------------------------
// Drop your schema file next to your Rust source and embed it at compile time
// — no runtime I/O, no missing-file errors.
const SCHEMA: &str = include_str!("schema.xml");

// ---------------------------------------------------------------------------
// 2. Define Rust Structs
// ---------------------------------------------------------------------------
// The struct (or enum variant) must carry `#[serde(rename = "<template_name>")]`
// so the decoder knows which XML template to match — the rename value must
// match the `name` attribute on `<template>` in the schema.

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename = "MarketData")]
struct MarketData {
    // Constant fields (MessageType) are never written on the wire and don't
    // need struct fields — their values come from the schema.
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
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

// Use a serde enum to cover multiple templates — the decoder always
// deserializes into an enum, even when the schema has only one template.
#[derive(Debug, serde::Deserialize)]
enum Message {
    #[serde(rename = "MarketData")]
    MarketData(MarketData),
    #[serde(rename = "TradeCapture")]
    TradeCapture(TradeCapture),
}

fn main() -> superfast::Result<()> {
    // -----------------------------------------------------------------------
    // 3. Decode — That's it, three lines to decode a FAST message
    // -----------------------------------------------------------------------
    let mut dec = FastDecoder::new(SCHEMA)?;

    // Encode a message first so we have bytes to decode (see step 4)
    let mut enc = FastEncoder::new(SCHEMA)?;

    let msg = MarketData {
        symbol: "AAPL".to_string(),
        sequence_number: 1,
        price: Decimal::from_string("172.50")?,
        volume: Some(1000),
        side: None,
        exchange: None,
    };

    // 4. Encode — template name comes from #[serde(rename = "MarketData")]
    let bytes = enc.encode(&msg)?;
    println!("Encoded {} bytes", bytes.len());

    // Decode from a buffer — returns (message, bytes_consumed)
    let (decoded, consumed): (Message, u64) = dec.decode(&bytes)?;
    let Message::MarketData(m) = decoded else {
        unreachable!()
    };
    println!(
        "symbol: {}, bid: {:.2}, consumed: {}",
        m.symbol,
        m.price.to_float(),
        consumed
    );

    // -----------------------------------------------------------------------
    // 5. Decoding Multiple Messages from a Concatenated Buffer
    // -----------------------------------------------------------------------
    let bytes2 = enc.encode(&MarketData {
        symbol: "AAPL".to_string(),
        sequence_number: 2,
        price: Decimal::from_string("173.00")?,
        volume: Some(2000),
        side: Some(1),
        exchange: Some("NASDAQ".to_string()),
    })?;

    let concatenated = [bytes, bytes2].concat();
    let mut offset = 0;
    while offset < concatenated.len() {
        let (msg, consumed): (Message, u64) = dec.decode(&concatenated[offset..])?;
        offset += consumed as usize;
        if let Message::MarketData(m) = msg {
            println!("  decoded: {} @ {:.2}", m.symbol, m.price.to_float());
        }
    }

    // -----------------------------------------------------------------------
    // 6. Decoding Multiple Template Types
    // -----------------------------------------------------------------------
    let trade = TradeCapture {
        trade_id: 1,
        symbol: "GOOG".to_string(),
        price: Decimal::from_string("9427.55")?,
        quantity: 100,
        timestamp: Some("2024-01-15T10:30:00".to_string()),
    };

    let trade_bytes = enc.encode(&trade)?;
    let (multi_msg, _): (Message, u64) = FastDecoder::new(SCHEMA)?.decode(&trade_bytes)?;
    match multi_msg {
        Message::MarketData(m) => println!("market data: {}", m.symbol),
        Message::TradeCapture(t) => println!("trade: {} qty={}", t.symbol, t.quantity),
    }

    // -----------------------------------------------------------------------
    // 7. Context Management
    // -----------------------------------------------------------------------
    // Stateful operators (copy, increment, tail, delta) maintain context
    // between messages. Reuse the same encoder/decoder instance across calls.
    let mut encoder = FastEncoder::new(SCHEMA)?;
    let mut decoder = FastDecoder::new(SCHEMA)?;

    // First message — full payload
    let bytes1 = encoder.encode(&MarketData {
        symbol: "MSFT".to_string(),
        sequence_number: 1,
        price: Decimal::from_string("380.00")?,
        volume: Some(500),
        side: Some(0),
        exchange: Some("NASDAQ".to_string()),
    })?;

    // Second message — compressed (copy/tail/increment operators skip unchanged fields)
    let bytes2 = encoder.encode(&MarketData {
        symbol: "MSFT".to_string(), // same symbol — tail saves bytes
        sequence_number: 2,         // sequential — increment skips
        price: Decimal::from_string("380.50")?,
        volume: Some(500),                    // unchanged — copy skips
        side: Some(0),                        // same default — default skips
        exchange: Some("NASDAQ".to_string()), // same — tail saves
    })?;

    println!("First message:  {} bytes", bytes1.len());
    println!(
        "Second message: {} bytes (compressed by {}%)",
        bytes2.len(),
        ((bytes1.len() as f64 - bytes2.len() as f64) / bytes1.len() as f64 * 100.0) as i32
    );
    assert!(bytes2.len() < bytes1.len());

    // Decode maintains state too
    let (m1, _): (Message, u64) = decoder.decode(&bytes1)?;
    let (m2, _): (Message, u64) = decoder.decode(&bytes2)?;
    if let (Message::MarketData(a), Message::MarketData(b)) = (m1, m2) {
        assert_eq!(a.price.to_float(), 380.0);
        assert_eq!(b.price.to_float(), 380.5);
        println!(
            "Round-trip verified: {:.2} -> {:.2}",
            a.price.to_float(),
            b.price.to_float()
        );
    }

    // -----------------------------------------------------------------------
    // 8. Using Decimal
    // -----------------------------------------------------------------------
    // FAST uses arbitrary-precision decimals — not floating-point.
    // A Decimal is stored as (exponent: i32, mantissa: i64), representing
    // mantissa * 10^exponent exactly.

    // From string — automatically normalizes trailing zeros
    let d = Decimal::from_string("9427.55")?; // exponent=-2, mantissa=942755
    println!("{} * 10^{}", d.mantissa, d.exponent);

    let d = Decimal::from_string("1000")?; // exponent=3, mantissa=1
    println!("{} * 10^{}", d.mantissa, d.exponent);

    // From float — for when you receive data as f64
    let d = Decimal::from_float(9427.55)?;
    println!("{}", d); // "9427.55"

    // Direct construction
    let d = Decimal::new(-2, 942755);
    println!("{}", d); // "9427.55"

    // Back to float
    let f = d.to_float();
    println!("{:.2}", f); // "9427.55"

    // Note: f32/f64 are rejected for FAST decimal fields — use Decimal
    // to avoid silent precision loss.
    println!("\nAll demo examples completed successfully!");

    Ok(())
}
