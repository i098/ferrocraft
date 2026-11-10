# world

Chunks, blocks, entities and physics, ported method by method from vanilla 1.8.9. Each port has a differential test against `oracle`, except the block ray and selection boxes in `dig` (only break speed is checked there), and the placement rules in `place` (checked against the local server by `engine/tests/place_server.rs`, which is `#[ignore]`d; see its header).

Owner: fc-scaffold (player movement physics only; other slices add their own modules)
