#![no_main]

use libfuzzer_sys::fuzz_target;
use tx_engine::process_tx;
use tx_engine::types::{Accounts, TxType};

fuzz_target!(|txs: Vec<TxType>| {
    let mut accounts = Accounts::default();

    // Process all transactions
    for tx in txs {
        process_tx(tx, &mut accounts);
    }

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

        // Held balance should never be negative
        assert!(
            account.held >= rust_decimal::Decimal::ZERO,
            "Held balance is negative: {}",
            account.held
        );

        // If account is locked, it should have had a chargeback
        if account.locked {
            assert!(
                account
                    .deposit_txs
                    .values()
                    .any(|d| matches!(d.dispute, tx_engine::types::DisputeState::Chargeback)),
                "Account is locked but has no chargebacks"
            );
        }
    }
});
