# Shared replication experience

- Validate comparator correctness before treating hit rates as protocol evidence.
  Missing/null fields are not numeric zero; zero must match a present zero.
  Require the full expected book length before element comparison, since zip
  silently ignores a longer side. Include negative regression cases for all three.
  After correcting a comparator, invalidate its affected historical metrics and
  recalculate with fixed input hashes; do not invalidate unrelated implementations.
  Report field-specific eligible denominators and missing counts, and distinguish
  selected-field matches from whole-record equality. Ratio diagnostics need their
  own presence checks even when the boolean equality helper is correct.

- A connected socket or zero frame errors does not prove business decoding.
- Test account 168 can reach 5188 and is useful for diagnostics, but formal
  account evidence remains the acceptance baseline.
- Compare subscriptions only within the same `0104` table version.
- Never infer token/mask fixes from OEM hit rate.
- Never use loose wall-clock nearest matching for callback parity. Match market,
  code, exact business second, and state group first.
- Keep capture packet loss, frame errors, partial prefixes, omitted tails,
  runtime rejection, and missing metadata as different metrics.
- `decoded-values.json[].record` is the 311-byte internal record. Read cumulative
  volume as signed little-endian `record[0x14:0x1c]`; order by manifest flow and
  frame sequence, not filename order.
- `OemState` presence and numeric zero are distinct concepts. Do not infer field
  presence solely from nonzero bytes without protocol evidence.
- An internal native 311-byte commit never authorizes public publication.
- WebClx build success proves only the commands in its build log.
- Index-stable cross-day `0104` is not a previous-close seed. On the 09:04
  auction overlapping join, the 09-03 table keeps names but drops
  `last_close` to 198/17,074; the same-day 09:09 table restores
  `last_close` 25,862/25,862.
- Callback field-hit totals include `0=0`. Auction 09:24 Wine zeros versus
  leftover internal prices inflate or deflate rates; report nonzero hits
  separately before calling dynamic parity closed.
