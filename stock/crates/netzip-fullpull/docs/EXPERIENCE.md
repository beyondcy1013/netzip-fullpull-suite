# Shared replication experience

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

