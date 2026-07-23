// No tests yet: the instruction handler CPIs into external programs
// (Bubblegum, SPL Account Compression) and a quasar-test world that loads
// those fixture binaries has not been written. The Anchor twin's LiteSVM
// suite covers the same flows. TODO: port that suite to quasar-test, loading
// the fixture .so files under ../anchor/tests/fixtures/ via
// test.add(Program::new(id, &std::fs::read(path).unwrap())).
