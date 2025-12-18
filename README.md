# Simple Transaction Engine

This is a simple transaction engine that simulates processing of financial transactions in "bank-like" style.

It reads the CSV with transaction entries and lazily processes them into user "bank" accounts.
It then outputs the list of account states in the CSV format into the STDOUT.

## Usage


```bash
cargo run -- <path-to-file.csv> > output.csv
```

## Tests


### Unit tests:
```bash
cargo test
```

### Fuzzing:

Setup:
```bash
rustup install nightly
cargo install cargo-fuzz
```

Run fuzz tests:

Fuzzed input transactions:
```bash
cargo +nightly fuzz run fuzz_transactions -- -max_total_time=FUZZ_TIME
```

Fuzzed input csv bytes:
```bash
cargo +nightly fuzz run fuzz_csv -- -max_total_time=FUZZ_TIME
```

## Assumptions

- Inputs entries are valid, if not, they are ignored
- Disputes can only happen on deposits, not withdrawals
- Resolved disputes reset deposit state. Hence, deposit transaction can be disputed again
- Disputes and chargeback can result in negative balances. That is, a dispute can happen if available balance is lower than disputed amount. Normally, this is how merchants accept losses 
- Chargeback results in account locking. This prevents further deposits and withdrawals, but does allow disputes on past transactions
