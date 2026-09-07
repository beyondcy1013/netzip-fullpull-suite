# Hypothesis ledger

## Open

- SZ000858 absolute accumulator corruption is caused by an incorrect absolute
  token operation, width/sign interpretation, or token consumption before
  internal offset `0x14` is written.
- Native EOF commits may represent correct native state evolution, but their
  business values and downstream propagation still require Wine-state parity.
- Zero-valued sparse fields need an explicit presence model; byte-nonzero merge
  behavior may retain stale values.

## Rejected

- Missing fresh baseline alone explains dirty quotes: rejected by the 5,000
  trace symbol-group comparison.
- OEM levels 6-10 are required upstream truth: rejected; they are compatibility
  zeros and do not drive ladder decoding.
- NativeWineClamp zero frame errors imply decoder completion: rejected by
  dynamic OEM business-field parity and runtime rejection evidence.

Each hypothesis promotion requires a falsifying test, evidence path, and a
focused regression test.

