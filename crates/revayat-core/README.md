# Revayat core APIs

`revayat-core` contains the native subtitle, editorial workspace, rendering, and
language utilities used by `revayat-subtitle`. The typed `Document`, `Cue`,
`ImportOptions`, and `workflow::review::donor` interfaces are preferred for new
Rust integrations. JSON adapters remain available for recorded compatibility
contracts; their marker objects are distinct from ordinary persisted JSON.

## Corrected JSON adapter boundaries

- `workflow::merge_donor` receives `base: &mut serde_json::Value`. On success it
  updates the marked base document's styles, fonts, and track metadata and returns
  the remapped donor cues. Append those cues to the updated document when needed.
  A refusal leaves the base value unchanged. Rust callers must pass a mutable
  reference; the earlier by-value signature discarded the required base updates.
- `workflow::contract::path_marker` returns `Result<Value, String>`. Propagate the
  error with `?`; paths that cannot be represented as UTF-8 are refused instead
  of silently substituting a different filename. `prepare` validates its work
  destination before creating directories or publishing any workspace.
- `workflow::contract_json::dump_json` serializes literal JSON. Use it for stored
  records, identity bytes, and report byte limits. Marker-looking objects such as
  `{"$tuple": [1]}` remain objects. `dump` is the explicit compatibility-marker
  serializer; its `sorted` option also applies to nested `$dict` markers.
- `effective_times` accepts only `ass` and `srt`, validates the supported interval,
  and refuses an ASS interval collapsed by centisecond quantization.

Generation recipe `14` and normalization recipe `13` identify the corrected
generation behavior. Rebuilding keeps prior sources, decisions, editions, and
image records intact; visual approval does not transfer to a new edition.

## Text encoding

Preflight and import decoding share one codec resolver. UTF-8 is strict; the
`utf-8-sig` label also removes an initial signature. Generic `utf-16` and `utf-32`
require a BOM, while explicit LE/BE labels use that endianness. Invalid units,
surrogates, and out-of-range UTF-32 scalars are refused. Source bytes are retained.

Latin-1 aliases select ISO-8859-1 byte mappings, including the C1 range. Explicit
Windows-1252 remains a separate choice. Supported legacy codecs use strict
`encoding_rs` decoding with no replacement characters. UTF-7, Python escape
codecs, unsupported OEM pages, and binary transforms fail preflight; the presence
of a label in an old Python codec registry is not evidence of native support.

## Reference material

- [encoding_rs Unicode and Latin-1 behavior](https://docs.rs/encoding_rs/latest/encoding_rs/)
- [Unicode UAX #9: matching directional formatting](https://www.unicode.org/reports/tr9/#BD9)
- [libass dialogue parser](https://github.com/libass/libass/blob/master/libass/ass_parse.c)
- [libass subtitle renderer](https://github.com/libass/libass/blob/master/libass/ass_render.c)
- [tempfile Builder naming contract](https://docs.rs/tempfile/latest/tempfile/struct.Builder.html#method.rand_bytes)
