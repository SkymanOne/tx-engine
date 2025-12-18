use super::*;
use crate::types::{ClientAccount, Deposit};
use rust_decimal::dec;
use std::collections::HashMap;

#[test]
fn test_parse_csv() {
    let mut reader = path_reader("all_types.csv").unwrap();
    let transactions: anyhow::Result<Vec<TxType>> = record_iter(&mut reader).collect();
    let transactions = transactions.expect("Failed to deserialize CSV");
    assert_eq!(transactions.len(), 5);
    let mut deposits = 0;
    let mut withdrawals = 0;
    let mut dispute = 0;
    let mut resolve = 0;
    let mut chargeback = 0;
    for tx in &transactions {
        println!("{:?}", tx);
        match tx {
            TxType::Deposit(_) => deposits += 1,
            TxType::Withdrawal(_) => withdrawals += 1,
            TxType::Dispute(_) => dispute += 1,
            TxType::Resolve(_) => resolve += 1,
            TxType::Chargeback(_) => chargeback += 1,
        }
    }

    assert_eq!(deposits, 1);
    assert_eq!(withdrawals, 1);
    assert_eq!(dispute, 1);
    assert_eq!(resolve, 1);
    assert_eq!(chargeback, 1);
}

#[test]
fn test_simple() {
    let mut reader = path_reader("simple_test.csv").unwrap();
    let transactions: anyhow::Result<Vec<TxType>> = record_iter(&mut reader).collect();
    let transactions = transactions.expect("Failed to deserialize CSV");
    let mut accounts = Accounts::default();

    for t in transactions {
        process_tx(t, &mut accounts);
    }

    assert_eq!(accounts.len(), 2);

    let account_1 = accounts.get(&1).expect("account 1 should exist");
    let account_2 = accounts.get(&2).expect("account 1 should exist");

    assert_eq!(
        *account_1,
        ClientAccount {
            available: dec!(1.5),
            held: dec!(0),
            total: dec!(1.5),
            locked: false,
            deposit_txs: HashMap::from([
                (
                    1,
                    Deposit {
                        amount: dec!(1),
                        dispute: DisputeState::None
                    }
                ),
                (
                    3,
                    Deposit {
                        amount: dec!(2),
                        dispute: DisputeState::None
                    }
                ),
            ])
        }
    );

    assert_eq!(
        *account_2,
        ClientAccount {
            available: dec!(2),
            held: dec!(0),
            total: dec!(2),
            locked: false,
            deposit_txs: HashMap::from([(
                2,
                Deposit {
                    amount: dec!(2),
                    dispute: DisputeState::None
                }
            )])
        }
    )
}

#[test]
fn test_dispute_resolve_chargeback_workflow() {
    use types::{BalanceChange, Dispute};

    let mut accounts = Accounts::default();
    let client_id = 1;

    // Step 1: Three deposits
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 2,
            amount: dec!(50.0),
        }),
        &mut accounts,
    );
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 3,
            amount: dec!(25.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(175.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(175.0));
    assert_eq!(account.locked, false);

    // Step 2: Dispute first two deposits
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(75.0));
    assert_eq!(account.held, dec!(100.0));
    assert_eq!(account.total, dec!(175.0));
    assert_eq!(account.locked, false);

    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 2,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(25.0));
    assert_eq!(account.held, dec!(150.0));
    assert_eq!(account.total, dec!(175.0));
    assert_eq!(account.locked, false);

    // Step 3: Resolve first dispute
    process_tx(
        TxType::Resolve(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(125.0));
    assert_eq!(account.held, dec!(50.0));
    assert_eq!(account.total, dec!(175.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::None
    );

    // Step 4: Chargeback second dispute (locks account)
    process_tx(
        TxType::Chargeback(Dispute {
            client: client_id,
            tx: 2,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(125.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(125.0));
    assert_eq!(account.locked, true);
    assert_eq!(
        account.deposit_txs.get(&2).unwrap().dispute,
        DisputeState::Chargeback
    );

    // Step 5: Try to deposit another amount (should be ignored - account locked)
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 4,
            amount: dec!(200.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(125.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(125.0));
    assert_eq!(account.locked, true);
    // Deposit was ignored
    assert!(
        account.deposit_txs.get(&4).is_none(),
        "Deposit should be ignored"
    );

    // Step 6: Dispute the third transaction
    // Note: Disputes are allowed on locked accounts (only deposits/withdrawals are blocked)
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 3,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(25.0));
    assert_eq!(account.total, dec!(125.0));
    assert_eq!(account.locked, true);
    assert_eq!(
        account.deposit_txs.get(&3).unwrap().dispute,
        DisputeState::Disputed
    );
}

#[test]
fn test_dispute_resolve_multiple_times() {
    use types::{BalanceChange, Dispute};

    let mut accounts = Accounts::default();
    let client_id = 1;

    // Step 1: One deposit
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);

    // Step 2: First dispute
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(100.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Step 3: First resolve
    process_tx(
        TxType::Resolve(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::None
    );

    // Step 4: Second dispute (same transaction)
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(100.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Step 5: Second resolve
    process_tx(
        TxType::Resolve(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::None
    );
}

#[test]
fn test_chargeback_is_final() {
    use types::{BalanceChange, Dispute};

    let mut accounts = Accounts::default();
    let client_id = 1;

    // Step 1: One deposit
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);

    // Step 2: Dispute the deposit
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(100.0));
    assert_eq!(account.total, dec!(100.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Step 3: Chargeback (locks account)
    process_tx(
        TxType::Chargeback(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(0.0));
    assert_eq!(account.locked, true);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Chargeback
    );

    // Step 4: Try to dispute again (should be ignored - already chargedback)
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(0.0));
    assert_eq!(account.locked, true);
    // State should still be Chargeback (dispute was ignored)
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Chargeback
    );

    // Step 5: Try to chargeback again (should be ignored - not disputed)
    process_tx(
        TxType::Chargeback(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(0.0));
    assert_eq!(account.locked, true);
    // State should still be Chargeback (second chargeback was ignored)
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Chargeback
    );
}

#[test]
fn test_dispute_after_withdrawal_negative_balance() {
    use types::{BalanceChange, Dispute};

    let mut accounts = Accounts::default();
    let client_id = 1;

    // Step 1: Deposit $100
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(100.0));

    // Step 2: Withdraw $75
    process_tx(
        TxType::Withdrawal(BalanceChange {
            client: client_id,
            tx: 2,
            amount: dec!(75.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(25.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(25.0));

    // Step 3: Dispute the original deposit (results in negative available)
    process_tx(
        TxType::Dispute(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    // Available goes negative because we withdrew $75 but now disputing the $100 deposit
    assert_eq!(account.available, dec!(-75.0));
    assert_eq!(account.held, dec!(100.0));
    assert_eq!(account.total, dec!(25.0));
    assert_eq!(account.locked, false);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Step 4: Chargeback (account goes into debt)
    process_tx(
        TxType::Chargeback(Dispute {
            client: client_id,
            tx: 1,
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    // Available stays at -$75, held goes to 0, total becomes -$75
    assert_eq!(account.available, dec!(-75.0));
    assert_eq!(account.held, dec!(0.0));
    assert_eq!(account.total, dec!(-75.0));
    assert_eq!(account.locked, true);
    assert_eq!(
        account.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Chargeback
    );

    // Verify invariant: total = available + held
    assert_eq!(
        account.total,
        account.available + account.held,
        "Invariant violated: total should equal available + held"
    );
}

#[test]
fn test_chargeback_different_client_ignored() {
    use types::{BalanceChange, Dispute};

    let mut accounts = Accounts::default();

    // Step 1: Client 1 deposits $100
    process_tx(
        TxType::Deposit(BalanceChange {
            client: 1,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );

    let account_1 = accounts.get(&1).unwrap();
    assert_eq!(account_1.available, dec!(100.0));
    assert_eq!(account_1.held, dec!(0.0));
    assert_eq!(account_1.total, dec!(100.0));

    // Step 2: Client 1 disputes their deposit
    process_tx(TxType::Dispute(Dispute { client: 1, tx: 1 }), &mut accounts);

    let account_1 = accounts.get(&1).unwrap();
    assert_eq!(account_1.available, dec!(0.0));
    assert_eq!(account_1.held, dec!(100.0));
    assert_eq!(account_1.total, dec!(100.0));
    assert_eq!(account_1.locked, false);
    assert_eq!(
        account_1.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Step 3: Client 2 tries to chargeback Client 1's transaction (should be ignored)
    process_tx(
        TxType::Chargeback(Dispute { client: 2, tx: 1 }),
        &mut accounts,
    );

    // Client 1's account should be unchanged
    let account_1 = accounts.get(&1).unwrap();
    assert_eq!(account_1.available, dec!(0.0));
    assert_eq!(account_1.held, dec!(100.0));
    assert_eq!(account_1.total, dec!(100.0));
    assert_eq!(account_1.locked, false);
    // Still disputed, not chargedback
    assert_eq!(
        account_1.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Disputed
    );

    // Client 2 should either not exist or have no balance
    // (the chargeback should have been ignored because client 2 has no tx 1)
    if let Some(account_2) = accounts.get(&2) {
        assert_eq!(account_2.available, dec!(0.0));
        assert_eq!(account_2.held, dec!(0.0));
        assert_eq!(account_2.total, dec!(0.0));
        assert_eq!(account_2.locked, false);
    }

    // Step 4: Client 1 performs legitimate chargeback
    process_tx(
        TxType::Chargeback(Dispute { client: 1, tx: 1 }),
        &mut accounts,
    );

    let account_1 = accounts.get(&1).unwrap();
    assert_eq!(account_1.available, dec!(0.0));
    assert_eq!(account_1.held, dec!(0.0));
    assert_eq!(account_1.total, dec!(0.0));
    assert_eq!(account_1.locked, true);
    assert_eq!(
        account_1.deposit_txs.get(&1).unwrap().dispute,
        DisputeState::Chargeback
    );
}

#[test]
fn test_insufficient_funds_withdrawal() {
    use types::BalanceChange;

    let mut accounts = Accounts::default();
    let client_id = 1;

    // Step 1: Deposit $100
    process_tx(
        TxType::Deposit(BalanceChange {
            client: client_id,
            tx: 1,
            amount: dec!(100.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(100.0));
    assert_eq!(account.total, dec!(100.0));

    // Step 2: First withdrawal of $60 (should succeed)
    process_tx(
        TxType::Withdrawal(BalanceChange {
            client: client_id,
            tx: 2,
            amount: dec!(60.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(40.0));
    assert_eq!(account.total, dec!(40.0));

    // Step 3: Second withdrawal of $50 (should fail - insufficient funds)
    process_tx(
        TxType::Withdrawal(BalanceChange {
            client: client_id,
            tx: 3,
            amount: dec!(50.0),
        }),
        &mut accounts,
    );

    // Balance should remain unchanged (withdrawal was ignored)
    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(40.0));
    assert_eq!(account.total, dec!(40.0));

    // Step 4: Withdrawal of exactly available amount (should succeed)
    process_tx(
        TxType::Withdrawal(BalanceChange {
            client: client_id,
            tx: 4,
            amount: dec!(40.0),
        }),
        &mut accounts,
    );

    let account = accounts.get(&client_id).unwrap();
    assert_eq!(account.available, dec!(0.0));
    assert_eq!(account.total, dec!(0.0));
}

#[test]
fn test_csv_output_precision_4dp() {
    use types::BalanceChange;

    let mut accounts = Accounts::default();

    // Create transactions that result in non-round numbers
    process_tx(
        TxType::Deposit(BalanceChange {
            client: 1,
            tx: 1,
            amount: dec!(100.12345),
        }),
        &mut accounts,
    );

    process_tx(
        TxType::Withdrawal(BalanceChange {
            client: 1,
            tx: 2,
            amount: dec!(25.6789),
        }),
        &mut accounts,
    );

    // Write to in-memory buffer
    let mut output = Vec::new();
    {
        let mut writer = WriterBuilder::new()
            .has_headers(true)
            .from_writer(&mut output);

        // Normalize all accounts and convert to AccountRow format
        let account_rows: Vec<AccountRow> = accounts
            .iter_mut()
            .map(|(client_id, account)| {
                account.normalize();
                AccountRow {
                    client: *client_id,
                    available: account.available,
                    held: account.held,
                    total: account.total,
                    locked: account.locked,
                }
            })
            .collect();

        for row in account_rows {
            writer.serialize(row).unwrap();
        }

        writer.flush().unwrap();
    }

    // Parse the CSV output
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.trim().lines().collect();

    // Should have header + 1 data row
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "client,available,held,total,locked");

    // Parse the data row
    let data_line = lines[1];
    let fields: Vec<&str> = data_line.split(',').collect();
    assert_eq!(fields.len(), 5);

    // Verify client ID
    assert_eq!(fields[0], "1");

    // Verify precision of decimal fields (available, held, total)
    // available = 100.12345 - 25.6789 = 74.44455 -> rounded to 74.4446
    let available = fields[1];
    let held = fields[2];
    let total = fields[3];

    assert_eq!(available, "74.4446", "Available should be rounded to 4dp");
    assert_eq!(total, "74.4446", "Total should be rounded to 4dp");

    // locked
    assert_eq!(fields[4], "false");

    // Verify all decimal values have at most 4 decimal places
    for (name, field) in [("available", available), ("held", held), ("total", total)] {
        if field.contains('.') {
            let decimal_part = field.split('.').nth(1).unwrap();
            assert!(
                decimal_part.len() <= 4,
                "{} decimal part should have at most 4 digits: {} (got {} digits)",
                name,
                field,
                decimal_part.len()
            );
        }
    }
}
