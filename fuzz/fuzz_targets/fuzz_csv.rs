#![no_main]

use csv::ReaderBuilder;
use libfuzzer_sys::fuzz_target;
use tx_engine::{process_txs, types::Accounts};

fuzz_target!(|data: &[u8]| {
    // Try to parse arbitrary bytes as CSV and process transactions
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(data);

    let mut accounts = Accounts::default();
    process_txs(&mut reader, &mut accounts);

    // Verify invariants after processing
    for (_, account) in accounts.iter() {
        // Total should equal available + held
        let calculated_total = account.available + account.held;
        // Allow for small rounding errors due to decimal operations
        let diff = (account.total - calculated_total).abs();
        assert!(
            diff < rust_decimal::Decimal::new(1, 4),
            "Invariant violated: total ({}) != available ({}) + held ({})",
            account.total,
            account.available,
            account.held
        );

        // Held should never be negative
        assert!(
            account.held >= rust_decimal::Decimal::ZERO,
            "Held balance is negative: {}",
            account.held
        );
    }
});
