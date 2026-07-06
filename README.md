# SuperFAST

> **FAST** — FIX Adapted for STreaming. **SuperFAST** — a zero-boilerplate, schema-driven FAST v1.1 encoder/decoder in Rust.

SuperFAST takes an XML template definition and binary data, then deserializes the bytes directly into your Rust structs via `serde`. No code generation. No hand-written parsers. The XML schema drives everything.

This library is a full implementation of the **[FAST Specification Version 1.1](./spec/FAST-Specification-1-x-1.pdf)**.

```rust
// That's it — three lines to decode a FAST message
let mut dec = FastDecoder::new(&xml)?;
let (msg, consumed): (MarketData, u64) = dec.decode(buffer)?;
println!("symbol: {}, bid: {}", msg.symbol, msg.bid_price);
```

## Why SuperFAST?

| Feature | What you get |
|---|---|
| **Schema-driven** | Define templates in XML, decode into `serde::Deserialize` structs |
| **All operators** | `copy`, `default`, `increment`, `delta`, `tail`, `constant` — spec-compliant |
| **Stateful context** | Encoder/decoder track field state across messages for compression operators |
| **Round-trip fidelity** | Encode → decode → encode produces identical bytes, even for truncated sequences |
| **Exact decimals** | Arbitrary-precision `Decimal(exponent, mantissa)` — no floating-point precision loss |
| **Zero codegen** | No build scripts, no macros — just XML + serde derives |

## Installation

Add to `Cargo.toml`:

```toml
[dependencies]
superfast = "0.1"
serde = { version = "1.0", features = ["derive"] }
```

## Quick Start

### 1. Embed Your XML Schema with `include_str!`

Drop your schema file next to your Rust source and embed it at compile time — no runtime I/O, no missing-file errors:

```rust
const SCHEMA: &str = include_str!("schema.xml");
```

**`schema.xml`** (a realistic example with all six operators):

```xml
<templates>
  <template id="100" name="MarketData">
    <!-- Constant: never written on the wire -->
    <uInt32 id="1" name="MessageType"><constant value="1"/></uInt32>

    <!-- No operator: mandatory, always written -->
    <string id="2" name="Symbol"/>

    <!-- Increment: omitted when value == previous + 1 -->
    <uInt64 id="3" name="SequenceNumber"><increment value="0"/></uInt64>

    <!-- Decimal: exact representation via exponent + mantissa -->
    <decimal id="4" name="Price"/>

    <!-- Copy: only written when value changes -->
    <uInt32 id="5" name="Volume" presence="optional"><copy value="0"/></uInt32>

    <!-- Default: omitted when value equals schema default -->
    <int32 id="6" name="Side" presence="optional"><default value="0"/></int32>

    <!-- Tail: only the new suffix is written -->
    <string id="7" name="Exchange" presence="optional"><tail/></string>
  </template>
</templates>
```

### 2. Define a Rust Struct

The struct (or enum variant) must carry `#[serde(rename = "<template_name>")]` so the
decoder knows which XML template to match — the rename value must match the
`name` attribute on `<template>` in the schema:

```rust
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename = "MarketData")]  // must match <template name="MarketData">
struct MarketData {
    #[serde(rename = "Symbol")]
    symbol: String,

    #[serde(rename = "SequenceNumber")]
    sequence_number: u64,

    #[serde(rename = "Price")]
    price: superfast::decimal::Decimal,

    #[serde(rename = "Volume", default)]
    volume: Option<u32>,

    #[serde(rename = "Side", default)]
    side: Option<i32>,

    #[serde(rename = "Exchange", default)]
    exchange: Option<String>,
}
```

> **Note:** Constant fields (`MessageType`) are never written on the wire and don't
> need struct fields — their values come from the schema.

### 3. Decode

```rust
use superfast::FastDecoder;

const SCHEMA: &str = include_str!("schema.xml");

let mut decoder = FastDecoder::new(SCHEMA)?;

// Decode from a buffer — returns (message, bytes_consumed)
let (msg, consumed): (MarketData, u64) = decoder.decode(buffer)?;
println!("{} @ {:.2}",
    msg.symbol, msg.price.to_float());
```

### 4. Encode

```rust
use superfast::{FastEncoder, decimal::Decimal};

const SCHEMA: &str = include_str!("schema.xml");

let mut encoder = FastEncoder::new(SCHEMA)?;

let msg = MarketData {
    symbol: "AAPL".to_string(),
    sequence_number: 1,
    price: Decimal::from_string("172.50")?,
    volume: Some(1000),
    side: None,
    exchange: None,
};

// Encode a struct — template name comes from #[serde(rename = "MarketData")]
let bytes = encoder.encode(&msg)?;
```

## Decoding

### Basic Decode

```rust
let (msg, consumed): (MarketData, u64) = decoder.decode(buffer)?;
```

The `consumed` value is the number of bytes read. To decode multiple messages from a concatenated buffer:

```rust
let mut offset = 0;
while offset < buffer.len() {
    let (msg, consumed): (MarketData, u64) = decoder.decode(&buffer[offset..])?;
    offset += consumed as usize;
    process(msg);
}
```

### Decoding Multiple Template Types

Use a serde enum to cover multiple templates:

```rust
#[derive(Debug, serde::Deserialize)]
enum Message {
    #[serde(rename = "MarketData")]
    MarketData(MarketData),
    #[serde(rename = "TradeCapture")]
    TradeCapture(TradeCapture),
}

let (msg, _): (Message, u64) = decoder.decode(buffer)?;
match msg {
    Message::MarketData(m) => { /* ... */ }
    Message::TradeCapture(t) => { /* ... */ }
}
```

## FAST Operators

FAST compression operators determine how much data is written to the wire. SuperFAST supports all six spec-defined operators:

### Copy — Only Write on Change

```xml
<uInt32 id="1" name="Seq" presence="optional"><copy value="0"/></uInt32>
```

The field is only written when its value differs from the previous message. First message always writes.

### Default — Only Write When Different

```xml
<int32 id="2" name="Status" presence="optional"><default value="0"/></int32>
```

The field is only written when it differs from the schema-defined default value.

### Increment — Only Write When Not Sequential

```xml
<uInt64 id="3" name="SeqNum" presence="mandatory"><increment value="0"/></uInt64>
```

The field is omitted when the value equals the previous value + 1 (ideal for sequence numbers).

### Delta — Write the Difference

```xml
<int64 id="4" name="Position"><delta value="0"/></int64>
```

Only the difference from the previous value is encoded. Works on integers and decimals.

### Tail — Write the New Suffix

```xml
<string id="5" name="Txt" presence="optional"><tail/></string>
```

For strings and byte vectors, only the replacement suffix after the common prefix is written. Unchanged strings produce zero payload bytes.

### Constant — Always the Same Value

```xml
<uInt32 id="6" name="MsgType"><constant value="42"/></uInt32>
```

Never written on the wire. The value comes from the schema definition.

## Data Types

| XML Element | Rust Type | Notes |
|---|---|---|
| `uInt32` | `u32` | Unsigned 32-bit integer |
| `int32` | `i32` | Signed 32-bit integer |
| `uInt64` | `u64` | Unsigned 64-bit integer |
| `int64` | `i64` | Signed 64-bit integer |
| `string` | `String` | ASCII by default; add `charset="unicode"` for UTF-8 |
| `byteVector` | `Vec<u8>` | Raw byte vector |
| `decimal` | `Decimal` | Arbitrary precision via `(exponent: i32, mantissa: i64)` |
| `sequence` | `Vec<T>` | Length-prefixed repeated items |
| `group` | Nested struct | Logical grouping of fields |
| `templateRef` | Referenced template | Static or dynamic template inclusion |

## Decimals

FAST uses arbitrary-precision decimals — not floating-point. A `Decimal` is stored as
`(exponent: i32, mantissa: i64)`, which represents `mantissa * 10^exponent` exactly.

### Why not `f64`?

Floating-point (`f32`/`f64`) cannot represent many common decimal values exactly:

| Value | `f64` representation |
|---|---|
| `0.1` | `0.1000000000000000055511151231257827...` |
| `172.50` | `172.499999999999971578...` |
| `9427.55` | `9427.54999999999927240...` |

Using `f64` for financial data introduces silent precision loss on every encode/decode cycle.
FAST avoids this by sending two integers — the wire format is exact.

SuperFAST enforces this: using `f32` or `f64` in a struct for a FAST `<decimal>` field
returns a clear error at encode time. Always use `Decimal`:

```rust
// correct — exact
#[serde(rename = "Price")]
price: superfast::decimal::Decimal,

// rejected — precision loss (f32 and f64)
#[serde(rename = "Price")]
price: f64,  // Error: "f32/f64 is not supported: use Decimal for FAST decimal fields"
```

### Using Decimal

```rust
use superfast::decimal::Decimal;

// From string — automatically normalizes trailing zeros
let d = Decimal::from_string("9427.55")?;   // exponent=-2, mantissa=942755
let d = Decimal::from_string("1000")?;      // exponent=3, mantissa=1

// From float — for when you receive data as f64
let d = Decimal::from_float(9427.55)?;

// Direct construction
let d = Decimal::new(-2, 942755);

// Back to float
let f = d.to_float(); // 9427.55

// Display
println!("{}", d);    // "9427.55"

// Inspect internal representation
println!("{} * 10^{}", d.mantissa, d.exponent);
```

## Advanced

### Context Management

Stateful operators (`copy`, `increment`, `tail`, `delta`) maintain context between messages. Reuse the same `FastDecoder`/`FastEncoder` instance across calls:

```rust
let mut decoder = FastDecoder::new(xml)?;
let mut encoder = FastEncoder::new(xml)?;

// First message — full payload
let bytes1 = encoder.encode(&msg1)?;
// Second message — compressed (copy/tail/increment operators skip unchanged fields)
let bytes2 = encoder.encode(&msg2)?;
assert!(bytes2.len() < bytes1.len());

// Decode maintains state too
let (m1, _): (MarketData, u64) = decoder.decode(&bytes1)?;
let (m2, _): (MarketData, u64) = decoder.decode(&bytes2)?;
```

### Handling Unknown Types on the Wire with `parse`

The standard `decode::<T>()` decodes directly into a known struct type. When the wire
carries multiple template types and you don't know which one comes next, use `parse()`
to get the intermediate `TemplateData` first — it reveals the template name so you can
dispatch, then deserialize into the correct known struct:

```rust
use superfast::{FastDecoder, FastEncoder};
use superfast::model::template::TemplateData;

// Parse — returns TemplateData for the first message in the buffer
let (td, consumed): (TemplateData, usize) = decoder.parse(buffer)?;

// Inspect the template name to dispatch
match td.name.as_str() {
    "NGTSTick" => {
        let msg: NgtsTick = td.decode()?;
        handle_tick(&msg);
    }
    "XtsTick" => {
        let msg: XtsTick = td.decode()?;
        handle_xts_tick(&msg);
    }
    _ => {} // unknown template
}
```

The fields are also accessible directly via convenience methods, so you can inspect
individual values before committing to deserialization:

```rust
let symbol = td.get_str("SecurityID");  // Some("600519")
let price  = td.get_i32("Price");       // Some(531)
let qty    = td.get_i64("Qty");         // Some(100_000)
```

### Complete Example: Parse then Deserialize

Here's a self-contained example that encodes a tick, parses it, and deserializes the
resulting `TemplateData` into a known struct:

```rust
use superfast::{FastDecoder, FastEncoder};
use superfast::model::template::TemplateData;
use serde::{Serialize, Deserialize};

const SCHEMA: &str = r#"<templates xmlns="http://www.fixprotocol.org/ns/template-definition">
  <template id="100" name="Tick">
    <string id="48" name="SecurityID"/>
    <int32 id="31" name="Price"/>
    <int64 id="32" name="Qty"/>
    <uInt64 id="1" name="SeqNum" presence="optional"><increment value="0"/></uInt64>
  </template>
</templates>"#;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "Tick")]
struct Tick {
    #[serde(rename = "SecurityID")] security_id: String,
    #[serde(rename = "Price")] price: i32,
    #[serde(rename = "Qty")] qty: i64,
    #[serde(rename = "SeqNum", default)] seq_num: Option<u64>,
}

fn main() {
    let mut enc = FastEncoder::new(SCHEMA).unwrap();
    let mut dec = FastDecoder::new(SCHEMA).unwrap();

    // Encode a tick
    let msg = Tick {
        security_id: "600519".into(),
        price: 531,
        qty: 100_000,
        seq_num: Some(1),
    };
    let bytes = enc.encode(&msg).unwrap();

    // Step 1: parse into TemplateData — reveals the template name
    let (td, _consumed): (TemplateData, usize) = dec.parse(&bytes).unwrap();
    assert_eq!(td.name, "Tick");

    // Step 2: inspect fields directly
    assert_eq!(td.get_str("SecurityID"), Some("600519"));
    assert_eq!(td.get_i32("Price"), Some(531));

    // Step 3: deserialize into the known struct type
    let decoded: Tick = td.decode().unwrap();
    assert_eq!(decoded, msg);
}
```

**When to use `parse`:**

| `decode::<T>()` | `parse()` → deserialize |
|---|---|
| Type known at compile time | Type determined at runtime by template name |
| One call, decode + deserialize | Two calls: parse → inspect → deserialize |
| Same struct for every message | Different struct per template type |
| Good for steady-state feeds | Good for multi-type streams or debugging |

## Architecture

```
┌─────────────────────────────────────────────────┐
│  XML Schema (templates, operators, types)       │
└────────────┬────────────────────────────────────┘
             │  parsed once at startup
             ▼
┌─────────────────────────────────────────────────┐
│  Definitions (instruction tree)                 │
└────┬───────────────┬────────────────────────────┘
     │               │
     ▼               ▼
┌─────────┐   ┌─────────────┐
│ Decoder │   │    Encoder   │
└────┬────┘   └──────┬──────┘
     │               │
     ▼               ▼
  bytes ←─────→ Typed structs (serde)
```

## Benchmark

SuperFAST is designed for high-throughput, low-latency workloads. Benchmarks encode and decode
100,000 messages per run (`cargo run --release --example benchmark`).

### Benchmark Machine

| Component | Specification |
|---|---|
| **CPU** | Dual Intel Xeon Gold 6338 @ 2.00 GHz (up to 3.2 GHz turbo) |
| **Cores** | 64 physical cores (128 threads, 2 sockets × 32 cores, HT enabled) |
| **Cache** | 96 MiB L3 per socket, 80 MiB L2, 3 MiB L1d + 2 MiB L1i |
| **Architecture** | x86_64, 2 NUMA nodes |

### Throughput

| Profile | Encode | Decode | Round-trip | µs/msg |
|---|---|---|---|---|
| **Steady stream** (copy/inc/tail) | 307k msgs/s | 367k msgs/s | 245k msgs/s | 4 |
| **Churning stream** (all fields change) | 301k msgs/s | 364k msgs/s | 243k msgs/s | 4 |
| **Trade captures** (mixed operators) | 347k msgs/s | 481k msgs/s | 310k msgs/s | 3 |

### Compression

Stateful operators (`copy`, `increment`, `tail`, `delta`) shrink wire size dramatically when fields don't change between messages:

| Profile | 1st msg | 50th msg | Reduction |
|---|---|---|---|
| **Steady stream** | 15 bytes | 9 bytes | 40% smaller |
| **Churning stream** | 15 bytes | 15 bytes | 0% (worst case) |
| **Trade captures** | 30 bytes | 13 bytes | 57% smaller |

Run the benchmarks yourself:

```bash
cargo run --release --example benchmark
```

## References

- [FAST Protocol Specification v1.1](./spec/FAST-Specification-1-x-1.pdf)
- Design goals: schema-driven, zero codegen, `serde` integration, full operator support