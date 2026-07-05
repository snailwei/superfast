# FAST v1.1 — Encoded Field Reference

**FIX Adapted for Streaming**: binary encoding for message-oriented data streams. Reduces size via (1) field operators that remove redundant data, and (2) stop-bit binary serialization with presence bitmaps.

**Error handling**: **Static** errors (template-only, e.g., malformed XML) — must signal, discard template. **Dynamic** errors (encoding/decoding time) — must signal. **Reportable** errors — encouraged but may be suppressed for performance; signal all during development/testing.

**Note**: The encoding/decoding model described in this spec is abstract. Implementations are free to differ internally as long as the result is the same as if this model was used. The concrete XML syntax below is one way to define templates — processors can also load templates via SCP (FAST Session Control Protocol) or have them hard-coded.

---

## 1. Stream Structure (EBNF)

```
stream ::= message* | block*
block  ::= BlockSize message+
message ::= segment
segment ::= PresenceMap TemplateIdentifier? (field | segment)*
field   ::= integer | string | delta | ScaledNumber | ByteVector
integer ::= UnsignedInteger | SignedInteger
string  ::= ASCIIString | UnicodeString
delta   ::= IntegerDelta | ScaledNumberDelta | ASCIIStringDelta | ByteVectorDelta
```

- **Messages** or **blocks** (framed by unsigned-integer byte-length preamble). The spec does not provide a way to indicate which style is used — producer/consumer must agree in advance.
- **Block** contains **at least one message**. Block size is the only integer allowed to be overlong. Error [ERR D12] if block size = 0.
- Each message is a **segment**: `PresenceMap [TemplateId] (field|sub-segment)*`.
- The extent of a segment is defined by the template and is dependent on the settings in the presence map.
- **Byte order**: big-endian. High-order bits and bytes first.

---

## 1.5 Application Types (Abstract Model)

FAST encodes instances of *application types* — abstract structures defined by the protocol using FAST:

- **Group**: a named type comprising an unordered set of fields.
- **Field**: has a name (unique within group) and a type (primitive, sequence, or group).
- **Sequence**: comprises a length and an ordered set of elements. Each element is of group type.
- **Primitive types**: ASCII string, Unicode string, uInt32, int32, uInt64, int64, decimal, byte vector.
- **Message**: a group appearing at the topmost level of a stream.

## 2. Stop-Bit Encoded Entities

All variable-length integers use stop-bit encoding:

- Each byte: bit 7 (MSB) = stop bit (1 = last byte), bits 0-6 = data.
- Entity value = concatenation of 7-bit data chunks of each byte.
- **NULL** (nullable types): 7-bit entity value of all zeros → `0x80` on wire.
- **Non-nullable by default**: unless explicitly specified (e.g., optional fields), non-nullable representations are used.
- **Overlong** (type-specific definitions):
  - **Integer**: entity value still represents the same integer after removing seven or more of the most significant data bits → reportable error [ERR R6]. Block size is the only integer where overlong is allowed.
  - **Presence map**: has more than seven bits and ends in seven or more bits that are all zero → reportable error [ERR R7].
  - **String**: starts with a zero-preamble, bits remain after removing it, and the first seven of those remaining bits are not all zero → reportable error [ERR R9].

### 2.1 Signed Integer

- Entity value = two's complement. MSB of entity value is sign bit.
- Sign-bit extension: if the sign bit of the value falls at a 7-bit boundary, emit extra 7-bit zeros/ones so the MSB is the sign bit. E.g., value 64 (`01000000`) encodes as `0x00 0xC0`.
- **Nullable** (applies to all integer types): every non-negative value is incremented by 1 before encoding. NULL = zero entity (`0x80`). Negative values are NOT incremented (their entity values are already non-zero due to two's complement sign extension).

### 2.2 Unsigned Integer

- Entity value = binary representation.
- NULL = zero entity (`0x80`) via the nullable increment rule in §2.1.
- **NOTE**: Nullable encoding may need more bits than the type's declared width. E.g., nullable uInt32 max (4294967295 → 4294967296) requires 33 significant bits: `0x10 0x00 0x00 0x00 0x80`.

### 2.3 ASCII String

- Entity value = sequence of 7-bit ASCII characters.
- **Zero-preamble**: a sequence of bits starting with seven zero bits. A string that starts with a zero-preamble consists of the bits that remain after removing the preamble.
- **Empty string**: zero-preamble (7 zero bits), then stop bit → `0x80` on wire.
- **Overlong**: starts with zero-preamble, remaining bits exist, and first 7 of those are **not all zero**. Reportable error [ERR R9].
- **Nullable**: extra zero-preamble at start → NULL if nothing follows.

| Entity value | Nullable | Description          |
|-------------|----------|----------------------|
| `0x00`      | —        | Empty string         |
| `0x00 0x00` | —        | `"\0"`               |
| `0x00 0x41` | —        | `"A"`, Overlong      |
| `0x00`      | Yes      | NULL                 |
| `0x00 0x00` | Yes      | Empty string         |
| `0x00 0x00 0x00` | Yes | `"\0"`         |
| `0x00 0x00 0x41`| Yes   | `"A"`, Overlong      |

### 2.4 Byte Vector

- **Unsigned-integer length** preamble, then raw bytes (8 data bits each — data part is NOT stop-bit encoded).
- For nullable byte vectors, the length preamble uses nullable unsigned-integer encoding (increment-by-1 for non-NULL).
- NULL = nullable length preamble is NULL.

### 2.5 Unicode String

- Byte Vector containing UTF-8 bytes. Nullable = nullable byte vector.

### 2.6 Scaled Number (Decimal)

- `value = mantissa × 10^exponent`
- Wire: **Signed Integer exponent**, then **Signed Integer mantissa**.
- Nullable: exponent is nullable; mantissa present IFF exponent is not NULL.
- Exponent range: [-63, 63]. Mantissa: int64.
- Entity value is always a multiple of seven bits; minimum length is seven bits.

### 2.7 Deltas

| Type              | Wire Format                                        | Nullable                        |
|-------------------|----------------------------------------------------|----------------------------------|
| Integer Delta     | Signed Integer                                     | Nullable signed integer         |
| Scaled Number Δ   | Signed Integer exp-δ, then Signed Integer mantissa-δ | Nullable exp-δ, non-nullable mantissa-δ |
| ASCII String Δ    | Signed Integer subtraction length, then ASCII String | Nullable subtraction length   |
| Byte Vector Δ     | Signed Integer subtraction length, then Byte Vector | Nullable subtraction length    |

**String/Byte Vector delta semantics**:

- Subtraction length uses **excess-1** encoding for zero: if decoded value is negative, increment by 1 to get characters/bytes to subtract. This makes it possible to encode negative zero as `-1`, meaning "add to front without removing any characters/bytes."
- Negative length = remove from **front**; positive = remove from **back**.
- New data is added to the **same end** as removal.
- Default base value: empty string / empty byte vector.
- Subtraction length must not exceed base length; error [ERR D7] if it does or if it exceeds int32 range.

**Unicode string delta**: structurally identical to byte vector delta. Operates on encoded UTF-8 bytes, not characters. Delta value may end in an incomplete UTF-8 sequence. Combined value must be valid UTF-8; error [ERR R2] otherwise.

**Integer delta overflow**: combined value must fit in the declared type; error [ERR R4] otherwise.
**NOTE**: The size of the integer required for the delta may be larger than the specified size for the field type. E.g., uInt32 base 4294967295 with new value 17 requires int64 to represent delta -4294967278. This does not affect how the delta appears in the stream.

**Decimal delta NOTE**: implementation must store the previous decimal value with exponent and mantissa parts preserved (or recoverable), as delta applies separate adjustments to each part.

---

## 3. Templates

Template defines field order, types, and operators. Order = wire order. Templates contain two categories of instructions: **field instructions** (encode fields) and **template reference instructions** (reuse other templates).

### 3.1 Field Instructions

| Element    | Type           | Attributes                    |
|------------|----------------|-------------------------------|
| `int32`    | signed int32   | `name`, `presence`, `id`      |
| `uInt32`   | unsigned int32 | `name`, `presence`, `id`      |
| `int64`    | signed int64   | `name`, `presence`, `id`      |
| `uInt64`   | unsigned int64 | `name`, `presence`, `id`      |
| `decimal`  | scaled number  | `name`, `presence`, `id`      |
| `string`   | ASCII/Unicode  | `name`, `presence`, `id`, `charset` ("ascii" default, "unicode"); unicode strings may have `<length>` child for byte vector length |
| `byteVector`| byte vector   | `name`, `presence`, `id`; may have `<length name="..."/>` child (type uInt32, handle for reporting length to application — does not affect wire encoding) |
| `sequence` | repeated group | `name`, `presence`, `dictionary`; optional `<length name="...">` child with operator |
| `group`    | field group    | `name`, `presence`, `dictionary` |

- `presence`: `"mandatory"` (default) or `"optional"`.

**Integer ranges**:

| Type   | Min                  | Max                  |
|--------|----------------------|----------------------|
| int32  | -2147483648          | 2147483647           |
| uInt32 | 0                    | 4294967295           |
| int64  | -9223372036854775808 | 9223372036854775807  |
| uInt64 | 0                    | 18446744073709551615 |

It is a dynamic error [ERR D2] if an integer in the stream falls outside these bounds for the specified type.

**Decimal**: exponent is int32, mantissa is int64. Exponent allowed range: [-63, 63]; reportable error [ERR R1] otherwise. Optional per-part operators: `<exponent><op/></exponent><mantissa><op/></mantissa>`. When operators are applied individually, the exponent and mantissa have generated names unique to the decimal field name — these names serve as default keys for the corresponding operators. Error [ERR D3] if a decimal value cannot be encoded due to individual operator limitations (e.g., constant exponent restricts representable values).

**Sequence**: optional `<length name="..." />` child with operator. Length = uInt32, appears before elements. Any encoding rule applicable to unsigned integer fields also applies to the length field. If sequence elements need presence-map bits, each element is represented as a **segment** (with its own presence map and fields). Length naming: **implicit** (generated, unique to sequence name, guaranteed non-colliding) when no `<length>` element; **explicit** when `<length name="...">` is present. When no `<length>` at all → implicit name, no operator. Elements are not required to have identical group types (heterogeneous sequences).

**Group**: single presence-map bit gates the whole group. If any instruction of the group needs a presence-map bit, the group is represented as a **segment** in the transfer encoding. Group is not required to have a corresponding notion in the application type — fields can be flattened. When absent (optional group, bit clear), previous values of fields inside are unaffected.

**TypeRef**: `<typeRef name="..." />` sets the current application type — an internal encode/decode context name for dictionary scoping. Appears at most once as the first child of `<template>`, `<sequence>`, or `<group>`. Not emitted on wire. The current application type is initially the special type `any`; it changes when a `<typeRef>` is encountered. Used by `dictionary="type"`: operators with `type` dictionary share base state across all templates referencing the same application type. Field type must be convertible to/from application type; dynamic error [ERR D1] otherwise.

**Instruction context** (encoding/decoding state):
- A set of templates
- A current template (changes via template identifier or static template reference)
- A set of application types
- A current application type (initially `any`)
- A set of dictionaries
- An optional initial value

### 3.2 Names

- Names = (namespace URI, local name). Namespace via `ns` attribute (inherited from nearest ancestor). Empty string if not specified.
- `templateNs` attribute: separate namespace for **template** names (not fields). Inherited the same way as `ns`. Two names are equal iff both namespace URI and local name match.
- Auxiliary `id` attribute — semantic meaning defined by the protocol (e.g., FIX tag numbers). Scope and semantics not defined by FAST spec.

### 3.3 Template Reference Instruction

- `<templateRef name="..." />` — **static** reference; no wire overhead. Error [ERR D8] if name not found.
- `<templateRef>` (no name) — **dynamic** reference; presence map + template ID on wire. Error [ERR D9] if ID unmatched.
- When the referred template ends, processing resumes after the reference and the current template is restored.

---

## 4. Field Operators

```
fieldOp = constant | default | copy | increment | delta | tail
```

Operators carry: `dictionary`, `key`, `value` (initial value), `ns`.

**Operator applicability** (error [ERR S2] if applied to unsupported type):

| Operator    | Applicable Types                           |
|-------------|--------------------------------------------|
| constant    | All field types                            |
| default     | All field types                            |
| copy        | All field types                            |
| increment   | Integer types only                         |
| delta       | Integer, decimal, string, byte vector      |
| tail        | String, byte vector only                   |

### 4.1 Dictionaries

Named dictionaries store previous values. Entry states: **undefined** (start), **assigned** (value present), **empty** (value absent — optional fields only).

| Dictionary | Scope                                         |
|------------|------------------------------------------------|
| `template` | Local to current template                      |
| `type`     | Local to current application type              |
| `global`   | Shared by all operators (default)              |
| custom     | Named string; operators with same name share   |

Default operator key = field name. Override with `key` attribute.
**Dictionary inheritance**: `dictionary` attribute is inherited from the nearest ancestor element. If not specified anywhere, the global dictionary is used.
**Dictionary reset**: resets all entries to undefined. Not defined by FAST spec how it's signaled — handled by the transport/session protocol. Resets must appear in the same order on encoder and decoder.
**Type match**: operator field type must match dictionary entry type; error [ERR D4] otherwise.

### 4.2 Initial Values

`value` attribute on operator → string converted to field type. Conversion errors treated as static errors [ERR S3]. Decimal initial values are **normalized** (`mant % 10 ≠ 0`; if mantissa is zero, both mantissa and exponent are zero).

### 4.3 Constant

`<constant value="X"/>` — field is always X. Never transferred.
- Mandatory: no presence-map bit.
- Optional: 1 bit (set = value present, clear = absent).
- Static error [ERR S4] if no initial value.

### 4.4 Default

`<default value="X"/>` — if not in stream, value = X.
- 1 presence-map bit. If clear → use initial value.
- Static error [ERR S5] on mandatory field without initial value.
- Optional field without initial value: field is considered absent when bit is clear.

### 4.5 Copy

`<copy/>` — value optionally in stream. If absent, reuse previous.

| Previous value state | Result when not in stream                    |
|----------------------|----------------------------------------------|
| assigned             | Value = previous value                       |
| undefined            | Value = initial value (becomes new previous). Error [ERR D5] if mandatory and no initial value; optional → empty. |
| empty                | Value absent (optional). Error [ERR D6] if mandatory. |

1 presence-map bit (mandatory and optional). Optional: NULL sets previous to empty.

### 4.6 Increment

`<increment value="1"/>` — like copy, but auto-increment by 1 when not in stream. Integer-only. Overflow wraps (max → min).

1 presence-map bit. Same undefined/empty semantics as copy.

### 4.7 Delta

`<delta value="base"/>` — wire carries the **difference** from base. The combined value becomes the new previous value.

**Base value**:
- Previous = assigned → base = previous
- Previous = undefined → base = initial value, or type default (0 for int/decimal, "" for string/byte vector)
- Previous = empty → error [ERR D6]

**No presence-map bit** — delta values are always physically in the stream. Optional fields use nullable representation; NULL = field absent (previous value left untouched, not set to empty).

**Integer delta**: `combined = base + delta` (signed integer). Default base = 0.
**Decimal delta**: separate deltas for exponent and mantissa. `combined_exp = base_exp + delta_exp`, `combined_mant = base_mant + delta_mant`. Default base = 0 (exponent=0, mantissa=0). Combined exponent must be in [-63, 63] and mantissa must fit int64; reportable error [ERR R1] otherwise.
**String delta**: subtraction length (signed int) + string data. Excess-1 encoding for zero.
**Byte vector delta**: same as string but on bytes. Also uses excess-1 encoding.

### 4.8 Tail

`<tail/>` — wire carries the **tail** (suffix replacement). String and byte vector only. The combined value becomes the new previous value.

| Previous state | Base value when tail IS in stream        |
|----------------|------------------------------------------|
| assigned       | Previous value                           |
| undefined      | Initial value, or type default           |
| empty          | Initial value, or type default           |

| Previous state | Value when tail NOT in stream            |
|----------------|------------------------------------------|
| assigned       | Previous value                           |
| undefined      | Initial value. Error [ERR D5] if mandatory and none. |
| empty          | Absent. Error [ERR D6] if mandatory.     |

**Tail semantics**: length of tail value = bytes/chars to remove from **back** of base, then append tail data. If tail length ≥ base length, result = tail value.

**Unicode string tail**: structurally identical to byte vector tail. Operates on encoded UTF-8 bytes, not characters. Tail value may end in an incomplete UTF-8 sequence. Combined value must be valid UTF-8; error [ERR R2] otherwise.

1 presence-map bit.

---

## 5. Presence Map & NULL Utilization

Presence map = stop-bit encoded entity. Logically infinite trailing zeros. Bits allocated in field order (earlier fields = higher-order bits).

**Overlong**: has more than seven bits and ends in seven or more bits that are all zero → error [ERR R7]. Too many bits → error [ERR R8].

### 5.1 Bit Allocation Table

| Operator    | Mandatory | Optional | Notes                                              |
|-------------|-----------|----------|-----------------------------------------------------|
| None        | No bit    | No bit   | Optional uses nullable wire encoding               |
| `<constant/>`| No bit   | 1 bit    | Bit=1 value present, bit=0 absent                  |
| `<copy/>`   | 1 bit     | 1 bit    |                                                    |
| `<default/>`| 1 bit    | 1 bit    |                                                    |
| `<delta/>`  | No bit    | No bit   | Optional uses nullable delta                       |
| `<increment/>`| 1 bit  | 1 bit    |                                                    |
| `<tail/>`   | 1 bit     | 1 bit    |                                                    |

### 5.2 Special Rules

- **Mandatory, no operator**: always in stream, no presence bit.
- **Optional, no operator**: nullable wire encoding, no presence bit. NULL = absent.
- **Optional with copy/increment/tail**: 1 bit. If set → nullable value in stream. NULL → previous = empty.
- **Optional with default**: 1 bit. If set → nullable value in stream. NULL → previous unchanged (not set to empty).
- **Delta**: no bit ever. Optional = nullable delta. NULL → previous value left untouched (not set to empty).
- **Group (optional)**: 1 bit. If clear → skip group entirely; previous values of fields inside are unaffected. If the application has no notion of groups, each field of an absent group is considered absent.
- **Decimal with individual operators**:
  - Mandatory decimal: exponent and mantissa are separate mandatory integer fields.
  - Optional decimal: exponent = optional, mantissa = mandatory. Mantissa present IFF exponent is present. Mantissa presence-map bit (if any) present IFF exponent is present.

### 5.3 Template Identifier

- A segment has a template identifier if it is a **message segment** or the result of a **dynamic template reference**.
- Encoded as unsigned integer with **copy** operator (global dictionary, internal key common to all template identifier fields).
- First bit in presence map allocated by template ID's copy operator.
- Not always physically present if ID unchanged from previous message.
- Error [ERR D9] if no template matches the identifier. This specification does not define how to map an identifier to a template name — implementations may use static allocation (auxiliary `id` attribute) or dynamic allocation (e.g., Session Control Protocol).
- Overlong template identifiers are reportable errors [ERR R6].

---

## 6. Type Conversion Rules

| From → To        | Rule                                                        | Error                    |
|------------------|-------------------------------------------------------------|--------------------------|
| String → Integer | Whitespace-trimmed digits, optional leading `-`.            | [ERR D11] syntax, [ERR R4] overflow |
| String → Decimal | `[−]integer.part.fractional.part` or `[−].fractional` or `[−]integer`. Normalize. | [ERR R1] exp range/mantissa overflow |
| String → Byte Vector | Hex digits [0-9A-Fa-f], whitespace stripped.        | [ERR D11]                |
| Integer → Integer| Cross-size allowed if no precision loss.                    | [ERR R4] (negative→unsigned) |
| Integer → Decimal| Exponent in [-63, 63], int64 mantissa.                      | [ERR R1]                 |
| Integer → String | Digits `0-9`, no leading zeros. Minus prefix if negative.   | —                        |
| Decimal → Integer| Only if no decimal part (exponent ≤ 0).                     | [ERR R5]                 |
| Decimal → String | Integer part + `.` + fractional, ≥1 digit each side. No leading zeros in integer part. If integer, convert as integer type. | — |
| Byte Vector → String | Hex digits [0-9a-f] (lowercase), even count.        | —                        |
| Byte Vector ↔ non-string | Forbidden.                                    | [ERR D10]                |
| ASCII → Unicode  | Trivial (ASCII ⊂ Unicode).                                  | —                        |
| Unicode → ASCII  | Only if all chars are ASCII.                                | [ERR R3]                 |

Whitespace chars: space (0x20), tab (0x09), CR (0x0D), LF (0x0A).
**Negative zero**: `-0` and `-0.0` normalize to positive. Cannot be represented in FAST stream.
**Initial values**: conversion errors on `value` attribute are treated as static errors [ERR S3].

---

## 7. Error Summary

| Code | Type       | Condition                                                    |
|------|------------|--------------------------------------------------------------|
| S1   | Static     | XML not well-formed, invalid namespace, or schema-invalid    |
| S2   | Static     | Operator not applicable to field type                        |
| S3   | Static     | Initial value cannot convert to field type                   |
| S4   | Static     | `<constant>` without initial value                           |
| S5   | Static     | `<default>` on mandatory field without initial value         |
| D1   | Dynamic    | Template field type not convertible to/from application type |
| D2   | Dynamic    | Integer out of bounds for specified type                     |
| D3   | Dynamic    | Decimal cannot encode due to individual operator limits      |
| D4   | Dynamic    | Operator entry type ≠ field type                             |
| D5   | Dynamic    | Mandatory field absent, undefined previous, no initial value |
| D6   | Dynamic    | Mandatory field absent with empty previous value             |
| D7   | Dynamic    | Subtraction length > base length or exceeds int32 range      |
| D8   | Dynamic    | Static template reference name not found                     |
| D9   | Dynamic    | Template identifier in stream has no matching template       |
| D10  | Dynamic    | Byte vector converted to/from non-string                     |
| D11  | Dynamic    | String syntax does not match target type rules               |
| D12  | Dynamic    | Block size = 0                                               |
| R1   | Reportable | Decimal exponent outside [-63, 63] or mantissa exceeds int64 |
| R2   | Reportable | Combined Unicode string is not valid UTF-8                   |
| R3   | Reportable | Unicode → ASCII conversion contains non-ASCII chars          |
| R4   | Reportable | Integer conversion overflow                                  |
| R5   | Reportable | Decimal → integer has fractional part or overflow            |
| R6   | Reportable | Overlong integer encoding                                    |
| R7   | Reportable | Overlong presence map                                        |
| R8   | Reportable | Presence map has more bits than needed                       |
| R9   | Reportable | Overlong string encoding                                     |

---

## 8. Encoding Examples

### Signed Integer (int32, mandatory): 942755
```
native:  0x0E 0x62 0xA3  (00001110 01100010 10100011)
FAST:    0x39 0x45 0xA3  (00111001 01000101 10100011)
                         stop bits: last byte MSB=1, propagated
```

### Signed Integer (int32, mandatory): 64 — sign-bit extension required
```
value:   01000000 (8 bits, MSB is sign=0)
entity:  0000000 1000000 (14 bits, sign-extended)
FAST:    0x00 0xC0  (00000000 11000000)
         sign bit extension needed so MSB of entity = sign bit
```

### Unsigned Integer (uInt32, optional): 0
```
value = 0 → increment to 1 → FAST: 0x81
NULL → FAST: 0x80
```

### ASCII String (mandatory): "ABC"
```
FAST: 0x41 0x42 0xC3  (01000001 01000010 11000011)
```

### ASCII String (mandatory): "" (empty)
```
FAST: 0x80
```

### Byte Vector (mandatory): [0x41, 0x42, 0x43]
```
length: 0x83 (3)  data: 0x41 0x42 0x43
```

### Decimal (mandatory): 94275500 = 942755 × 10²
```
exponent: 0x82 (2)  mantissa: 0x39 0x45 0xA3 (942755)
```

### Decimal (mandatory): 9427.55 = 942755 × 10⁻²
```
exponent: 0xFE (-2)  mantissa: 0x39 0x45 0xA3 (942755)
```

### Delta — String (mandatory): base="GEH6", new="GEM6"
```
Remove 2 from back ("H6"), append "M6" → delta = (2, "M6")
subtraction length: 0x82 (2)  string: 0x4D 0xB6 ("M6")
```

### Delta — String: base="GEM6", new="ESM6"
```
Remove 2 from front ("GE"), append "ES" at front → delta = (-2, "ES")
subtraction length: 0xFD (-2 with sign extension)  string: 0x45 0xD3 ("ES")
```

### Delta — String: base="ESM6", new="RSESM6" (excess-1 encoding)
```
Remove 0 from front, append "RS" at front → subtraction = -0 → encoded as -1 (excess-1)
subtraction length: 0xFF (-1)  string: 0x52 0xD3 ("RS")
Negative zero (-0) uses excess-1 encoding: encoded as -1, meaning "add to front without removing any characters."
```

### Copy — String sequence:
```
Message 1: input="CME", prior=none  → pmap=1, wire="CME" (0x43 0x4D 0xC5)
Message 2: input="CME", prior="CME" → pmap=0, wire=none (reuse prior)
Message 3: input="ISE", prior="CME" → pmap=1, wire="ISE" (0x49 0x53 0xC5)
```

### Increment — Integer sequence:
```
init=1, input=1 → pmap=0 (undefined→initial, prior=1)
      input=2 → pmap=0 (prior+1=2, prior=2)
      input=4 → pmap=1, wire=0x84 (4 encoded, prior=4)
      input=5 → pmap=0 (prior+1=5, prior=5)
```

---

## 9. XML Concrete Syntax (RELAX NG Summary)

```
start          = templates | template
templates      = <templates> { ns?, templateNs?, dictionary?, template* }
template       = <template> { templateNsName, ns?, dictionary?, typeRef?, instruction* }
typeRef        = <typeRef> { name, ns? }
instruction    = field | templateRef
field          = integer | decimal | string | byteVector | sequence | group
fieldOp        = constant | default | copy | increment | delta | tail
decFieldOp     = exponent?, mantissa?
exponent       = <exponent> { fieldOp }
mantissa       = <mantissa> { fieldOp }
constant       = <constant> { value }
default        = <default> { value? }
copy           = <copy>     { dictionary?, key?, ns?, value? }
increment      = <increment>{ dictionary?, key?, ns?, value? }
delta          = <delta>    { dictionary?, key?, ns?, value? }
tail           = <tail>     { dictionary?, key?, ns?, value? }
sequence       = <sequence> { name, presence?, dictionary?, typeRef?, length?, instruction* }
group          = <group>    { name, presence?, dictionary?, typeRef?, instruction* }
length         = <length>   { name?, fieldOp? }
stringLength   = <length>   { name, ns?, id? }          /* for unicode strings and byte vectors */
templateRef    = <templateRef> { name?, templateNs? }
```

Namespace URI: `http://www.fixprotocol.org/ns/fast/td/1.1` (prefix: `td:`)

### 9.1 Extensibility

Foreign elements/attributes may appear on any element. No restrictions on content. Foreign children may be placed freely.
- **Foreign attribute**: namespace URI ≠ empty string AND ≠ TD namespace URI.
- **Foreign element**: namespace URI ≠ TD namespace URI.
- Implemented via `other = foreignAttr*, foreignElm*` placed at relevant schema locations.

### 9.2 Template Reference

- **Static** (`<templateRef name="..." />`): processing continues with the referred template as the current template. Does NOT imply a presence map or template identifier on the wire. Error [ERR D8] if no template exists with the specified name.
- **Dynamic** (`<templateRef>` without `name`): a presence map and template identifier are present in the stream (represented as a segment). Processing continues with the template indicated by the identifier. Error [ERR D9] if no template matches.
- When processing reaches the end of the referred template (static or dynamic), it continues after the referring instruction and the current template is restored.

### 9.3 Minimal Template Covering All XML Features

```xml
<templates xmlns="http://www.fixprotocol.org/ns/fast/td/1.1"
           templateNs="http://example.com/templates"
           ns="http://example.com/fix"
           dictionary="global">

  <!-- ---- Header fragment (reused via static templateRef) ---- -->
  <template name="Header">
    <string name="BeginString">
      <constant value="FIX4.4"/>
    </string>
    <string name="MessageType">
      <constant value="X"/>
    </string>
    <uInt32 name="MsgSeqNum">
      <increment value="1"/>
    </uInt32>
    <string name="SenderCompID">
      <copy/>
    </string>
  </template>

  <!-- ---- Main message template ---- -->
  <template name="MarketData" id="100">
    <typeRef name="MarketDataIncrementalRefresh"/>

    <!-- Reuse header via static template reference -->
    <templateRef name="Header"/>

    <!-- Mandatory field, no operator → always on wire, no pmap bit -->
    <uInt64 name="Timestamp"/>

    <!-- Optional field, no operator → nullable wire encoding, no pmap bit -->
    <int64 name="SeqNo" presence="optional"/>

    <!-- Default operator → pmap bit; if clear, use initial value -->
    <int32 name="MarketSegment" presence="optional">
      <default value="0"/>
    </int32>

    <!-- Delta operator → no pmap bit; always on wire -->
    <decimal name="BidPrice">
      <delta value="100"/>
    </decimal>

    <!-- Optional delta → nullable wire encoding, no pmap bit -->
    <decimal name="AskPrice" presence="optional">
      <delta/>
    </decimal>

    <!-- Decimal with individual exponent/mantissa operators -->
    <decimal name="LastPrice">
      <exponent><copy dictionary="template"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>

    <!-- Copy with custom dictionary, explicit key, initial value -->
    <string name="Symbol">
      <copy dictionary="symDict" key="sym" value=""/>
    </string>

    <!-- Tail operator (string only) → pmap bit -->
    <string name="SecurityDesc">
      <tail/>
    </string>

    <!-- Unicode string with explicit length handle -->
    <string name="Note" charset="unicode">
      <length name="NoteLength"/>
      <copy/>
    </string>

    <!-- Byte vector with length handle and delta operator -->
    <byteVector name="RawData">
      <length name="RawDataLength"/>
      <delta/>
    </byteVector>

    <!-- Optional group — single pmap bit gates all children -->
    <group name="ExtendedAttributes" presence="optional"
           dictionary="type" ns="http://example.com/ext">
      <string name="AttrKey">
        <copy/>
      </string>
      <string name="AttrValue">
        <copy/>
      </string>
    </group>

    <!-- Sequence with explicit-length name and operator -->
    <sequence name="MDEntries" presence="optional">
      <length name="NoMDEntries">
        <delta/>
      </length>
      <uInt32 name="MDUpdateAction">
        <copy/>
      </uInt32>
      <decimal name="MDEntryPx">
        <delta/>
      </decimal>
      <decimal name="MDEntrySize">
        <delta/>
      </decimal>
    </sequence>

  </template>
</templates>
```

**Features covered by this example**:

| # | Feature | Element / Attribute |
|---|---------|---------------------|
| 1 | Template collection | `<templates>` with `ns`, `templateNs`, `dictionary` |
| 2 | Template identity | `<template name="..." id="...">` |
| 3 | Application type reference | `<typeRef name="..."/>` |
| 4 | Static template reference | `<templateRef name="Header"/>` |
| 5 | int32, uInt32, int64, uInt64 | all four integer field types |
| 6 | Decimal — single operator | `<decimal><delta/></decimal>` |
| 7 | Decimal — individual operators | `<exponent><copy/></exponent><mantissa><delta/></mantissa>` |
| 8 | ASCII string (default charset) | `<string name="Symbol">` |
| 9 | Unicode string | `<string charset="unicode">` |
| 10 | Byte vector | `<byteVector>` |
| 11 | Length handle | `<length name="NoteLength"/>` |
| 12 | Sequence | `<sequence>` with `<length>` |
| 13 | Group | `<group presence="optional">` |
| 14 | Mandatory presence | default (no attribute) |
| 15 | Optional presence | `presence="optional"` |
| 16 | `<constant>` | requires `value` |
| 17 | `<default>` | optional `value` |
| 18 | `<copy>` | with `dictionary`, `key`, `value` |
| 19 | `<increment>` | with `value` |
| 20 | `<delta>` | with `value` (initial base) |
| 21 | `<tail>` | string/byte vector only |
| 22 | Dictionary scoping | `dictionary="global"`, `"template"`, `"type"`, custom string |
| 23 | Namespace inheritance | `ns` on `<templates>`, overridden on `<group>` |
| 24 | Auxiliary identifier | `id="100"` on template |
